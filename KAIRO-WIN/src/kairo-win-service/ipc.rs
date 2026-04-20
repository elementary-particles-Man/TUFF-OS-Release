use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use crate::config::{load_acl_from_path, load_acl_from_str, DEFAULT_ACL_PATH};
use crate::ingress_bridge::{
    check_listen_allowed, on_accept_event, on_recv_event, IngressDecision,
};
use crate::khp::{
    decode_check_listen, encode_check_listen_resp, proto_u8_to_str, CheckListenResp, KHP_VERSION,
    MSG_CHECK_LISTEN,
};
use crate::state::KairoState;
use crate::tracker::global_tracker;
use crate::witness_ext::{log_extended, CauseCode};
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub const IPC_SOCK_ENV: &str = "KAIRO_SOCK";
pub const IPC_RUNTIME_DIR_ENV: &str = "KAIRO_RUNTIME_DIR";
pub const IPC_SOCKET_NAME: &str = "kairo.sock";

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    SwitchState {
        on: bool,
    },
    UpdateAcl {
        lines: Option<Vec<String>>,
        path: Option<String>,
    },
    SetUnknownMode {
        mode: String,
    },
    RegisterAgentPid {
        pid: u32,
    },
    UnregisterAgentPid {
        pid: u32,
    },
    MapPortPid {
        port: u16,
        pid: u32,
    },
    UnmapPort {
        port: u16,
    },
    OnSocket {
        fd: i32,
        pid: u32,
    },
    OnConnect {
        fd: i32,
        dst: String,
        src_port: u16,
    },
    OnClose {
        fd: i32,
    },
    OnAccept {
        fd: i32,
        pid: u32,
        remote: String,
        local_port: u16,
    },
    OnRecv {
        fd: i32,
        bytes: usize,
    },
    CheckListen {
        pid: u32,
        port: u16,
        proto: String,
    },

    // TUFF-OS Network Management Extensions
    BlacklistList,
    BlacklistEdit {
        file_path: String,
    },
    BlacklistRefresh {
        file_path: String,
    },
    AiList,
    AiEdit {
        file_path: String,
    },
    AiRefresh {
        file_path: String,
    },
    AiOn {
        password: Option<String>,
    },
    AiOff,
    AiPasswd {
        old_pass: String,
        new_pass: String,
    },
}

#[derive(Debug, Serialize)]
pub struct IpcReply {
    pub ok: bool,
    pub message: String,
}

pub async fn spawn_ipc_listener(state: Arc<KairoState>) -> Result<(), String> {
    let socket_path = resolve_ipc_socket_path();
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).map_err(|e| e.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|e| e.to_string())?;
    log::info!("IPC listener bound at {}", socket_path.display());

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let state = state.clone();
            tokio::spawn(async move {
                let (mut read_half, mut write_half) = stream.into_split();
                let mut buf = vec![0u8; 4096];
                let n = match read_half.read(&mut buf).await {
                    Ok(n) => n,
                    Err(_) => return,
                };
                if n == 0 {
                    return;
                };
                let req = &buf[..n];
                let first = req.iter().copied().find(|b| !b.is_ascii_whitespace());
                if matches!(first, Some(b'{')) {
                    let text = String::from_utf8_lossy(req);
                    let reply = match serde_json::from_str::<IpcCommand>(text.trim()) {
                        Ok(cmd) => handle_command(&state, cmd),
                        Err(e) => IpcReply {
                            ok: false,
                            message: format!("invalid command: {}", e),
                        },
                    };
                    let _ = write_half
                        .write_all(
                            format!("{}\n", serde_json::to_string(&reply).unwrap()).as_bytes(),
                        )
                        .await;
                } else {
                    let out = handle_khp(&state, req);
                    let _ = write_half.write_all(&out).await;
                }
            });
        }
    });
    Ok(())
}

pub fn resolve_ipc_socket_path() -> PathBuf {
    resolve_socket_path_from(
        std::env::var_os(IPC_SOCK_ENV),
        std::env::var_os(IPC_RUNTIME_DIR_ENV),
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("TMPDIR"),
    )
}

fn resolve_socket_path_from(
    explicit_socket: Option<std::ffi::OsString>,
    explicit_runtime_dir: Option<std::ffi::OsString>,
    xdg_runtime_dir: Option<std::ffi::OsString>,
    tmpdir: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(path) = non_empty_path(explicit_socket) {
        return path;
    }

    let runtime_dir = non_empty_path(explicit_runtime_dir)
        .or_else(|| non_empty_path(xdg_runtime_dir))
        .or_else(|| non_empty_path(tmpdir))
        .unwrap_or_else(|| PathBuf::from("/tmp/tuff-kairo"));
    runtime_dir.join(IPC_SOCKET_NAME)
}

fn non_empty_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.and_then(|raw| {
        if raw.is_empty() {
            None
        } else {
            Some(PathBuf::from(raw))
        }
    })
}

fn handle_khp(state: &KairoState, req: &[u8]) -> Vec<u8> {
    if req.len() < 8 {
        log_extended(
            crate::secure_log::Direction::Internal,
            "khp",
            IPC_SOCKET_NAME,
            "",
            CauseCode::HookRejected,
        );
        return encode_check_listen_resp(CheckListenResp { decision: 0 });
    }
    let hdr = match crate::khp::KhpHeader::decode(&req[..8]) {
        Ok(h) => h,
        Err(_) => {
            log_extended(
                crate::secure_log::Direction::Internal,
                "khp",
                IPC_SOCKET_NAME,
                "",
                CauseCode::HookRejected,
            );
            return encode_check_listen_resp(CheckListenResp { decision: 0 });
        }
    };
    if hdr.version != KHP_VERSION || hdr.msg_type != MSG_CHECK_LISTEN {
        log_extended(
            crate::secure_log::Direction::Internal,
            "khp",
            IPC_SOCKET_NAME,
            "",
            CauseCode::HookRejected,
        );
        return encode_check_listen_resp(CheckListenResp { decision: 0 });
    }
    let end = 8usize.saturating_add(hdr.msg_len as usize);
    if end > req.len() {
        log_extended(
            crate::secure_log::Direction::Internal,
            "khp",
            IPC_SOCKET_NAME,
            "",
            CauseCode::HookRejected,
        );
        return encode_check_listen_resp(CheckListenResp { decision: 0 });
    }
    let payload = &req[8..end];
    let cl = match decode_check_listen(payload) {
        Ok(v) => v,
        Err(_) => {
            log_extended(
                crate::secure_log::Direction::Internal,
                "khp",
                IPC_SOCKET_NAME,
                "",
                CauseCode::HookRejected,
            );
            return encode_check_listen_resp(CheckListenResp { decision: 0 });
        }
    };
    let proto = proto_u8_to_str(cl.proto);
    let allow = if let Some(t) = global_tracker() {
        matches!(
            check_listen_allowed(state, &t, cl.pid, cl.port, proto),
            IngressDecision::Allow
        )
    } else {
        false
    };
    encode_check_listen_resp(CheckListenResp {
        decision: if allow { 1 } else { 0 },
    })
}

fn handle_command(state: &KairoState, cmd: IpcCommand) -> IpcReply {
    match cmd {
        IpcCommand::SwitchState { on } => {
            if on {
                if let Err(reason) = state.can_switch_on() {
                    return IpcReply {
                        ok: false,
                        message: format!("switch ON blocked: {}", reason),
                    };
                }
            }
            state.set_enabled(on);
            IpcReply {
                ok: true,
                message: format!("state switched to {}", if on { "ON" } else { "OFF" }),
            }
        }
        IpcCommand::UpdateAcl { lines, path } => {
            let loaded = if let Some(lines) = lines {
                load_acl_from_str(&lines.join("\n"))
            } else {
                let p = path.unwrap_or_else(|| DEFAULT_ACL_PATH.to_string());
                load_acl_from_path(std::path::Path::new(&p))
            };
            match loaded {
                Ok(loaded) => {
                    state.swap_rules(loaded.rules);
                    state.mark_acl_status(true, loaded.allow_count);
                    IpcReply {
                        ok: true,
                        message: "acl updated atomically".to_string(),
                    }
                }
                Err(e) => {
                    state.mark_acl_status(false, 0);
                    IpcReply {
                        ok: false,
                        message: format!("acl parse failed: {}", e),
                    }
                }
            }
        }
        IpcCommand::SetUnknownMode { mode } => {
            let m = mode.to_ascii_lowercase();
            if m == "drop_and_force_off" {
                state.set_unknown_mode(crate::state::UnknownMode::DropAndForceOff);
                IpcReply {
                    ok: true,
                    message: "unknown mode set to drop_and_force_off".to_string(),
                }
            } else if m == "drop_only" {
                state.set_unknown_mode(crate::state::UnknownMode::DropOnly);
                IpcReply {
                    ok: true,
                    message: "unknown mode set to drop_only".to_string(),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "invalid mode".to_string(),
                }
            }
        }
        IpcCommand::RegisterAgentPid { pid } => {
            if let Some(t) = global_tracker() {
                t.register_agent_pid(pid);
                IpcReply {
                    ok: true,
                    message: format!("registered agent pid {}", pid),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::UnregisterAgentPid { pid } => {
            if let Some(t) = global_tracker() {
                t.unregister_agent_pid(pid);
                IpcReply {
                    ok: true,
                    message: format!("unregistered agent pid {}", pid),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::MapPortPid { port, pid } => {
            if let Some(t) = global_tracker() {
                t.map_source_port(port, pid);
                IpcReply {
                    ok: true,
                    message: format!("mapped port {} to pid {}", port, pid),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::UnmapPort { port } => {
            if let Some(t) = global_tracker() {
                t.unmap_source_port(port);
                IpcReply {
                    ok: true,
                    message: format!("unmapped port {}", port),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::OnSocket { fd, pid } => {
            if let Some(t) = global_tracker() {
                t.on_socket(fd, pid);
                IpcReply {
                    ok: true,
                    message: format!("socket tracked fd={} pid={}", fd, pid),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::OnConnect { fd, dst, src_port } => {
            if let Some(t) = global_tracker() {
                match dst.parse::<SocketAddr>() {
                    Ok(addr) => {
                        t.on_connect(fd, addr, src_port);
                        IpcReply {
                            ok: true,
                            message: format!("connect tracked fd={} dst={}", fd, dst),
                        }
                    }
                    Err(e) => IpcReply {
                        ok: false,
                        message: format!("invalid dst socket addr: {}", e),
                    },
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::OnClose { fd } => {
            if let Some(t) = global_tracker() {
                t.on_close(fd);
                IpcReply {
                    ok: true,
                    message: format!("flow closed fd={}", fd),
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::OnAccept {
            fd,
            pid,
            remote,
            local_port,
        } => {
            if let Some(t) = global_tracker() {
                match remote.parse::<SocketAddr>() {
                    Ok(addr) => {
                        t.on_accept(fd, pid, addr, local_port);
                        match on_accept_event(state, &t, fd, addr) {
                            IngressDecision::Allow => IpcReply {
                                ok: true,
                                message: format!("accept allowed fd={} remote={}", fd, remote),
                            },
                            IngressDecision::DropOnly { note } => IpcReply {
                                ok: false,
                                message: format!("accept dropped: {}", note),
                            },
                            IngressDecision::DropAndForceOff { note } => IpcReply {
                                ok: false,
                                message: format!("accept breaker: {}", note),
                            },
                        }
                    }
                    Err(e) => IpcReply {
                        ok: false,
                        message: format!("invalid remote socket addr: {}", e),
                    },
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::OnRecv { fd, bytes } => {
            if let Some(t) = global_tracker() {
                match on_recv_event(state, &t, fd, bytes) {
                    IngressDecision::Allow => {
                        let total = t.on_recv(fd, 0).unwrap_or(0);
                        IpcReply {
                            ok: true,
                            message: format!(
                                "recv allowed fd={} total_ingress_bytes={}",
                                fd, total
                            ),
                        }
                    }
                    IngressDecision::DropOnly { note } => IpcReply {
                        ok: false,
                        message: format!("recv dropped: {}", note),
                    },
                    IngressDecision::DropAndForceOff { note } => IpcReply {
                        ok: false,
                        message: format!("recv breaker: {}", note),
                    },
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }
        IpcCommand::CheckListen { pid, port, proto } => {
            if let Some(t) = global_tracker() {
                match check_listen_allowed(state, &t, pid, port, &proto.to_ascii_uppercase()) {
                    IngressDecision::Allow => IpcReply {
                        ok: true,
                        message: format!("listen allowed pid={} port={}", pid, port),
                    },
                    IngressDecision::DropOnly { note } => IpcReply {
                        ok: false,
                        message: format!("listen denied: {}", note),
                    },
                    IngressDecision::DropAndForceOff { note } => IpcReply {
                        ok: false,
                        message: format!("listen breaker: {}", note),
                    },
                }
            } else {
                IpcReply {
                    ok: false,
                    message: "tracker unavailable".to_string(),
                }
            }
        }

        // --- TUFF-OS Network Management Implementation ---
        IpcCommand::BlacklistList => match read_ssd_list("blacklist.txt") {
            Ok(list) => IpcReply {
                ok: true,
                message: list.join("\n"),
            },
            Err(e) => IpcReply {
                ok: false,
                message: format!("Failed to read blacklist: {}", e),
            },
        },
        IpcCommand::BlacklistEdit { file_path } => {
            match export_to_file("blacklist.txt", &file_path) {
                Ok(_) => IpcReply {
                    ok: true,
                    message: format!("Blacklist exported to {}", file_path),
                },
                Err(e) => IpcReply {
                    ok: false,
                    message: format!("Export failed: {}", e),
                },
            }
        }
        IpcCommand::BlacklistRefresh { file_path } => {
            match import_from_file(&file_path, "blacklist.txt") {
                Ok(_) => {
                    log_extended(
                        crate::secure_log::Direction::Internal,
                        "nw",
                        "blacklist",
                        "refreshed",
                        CauseCode::PolicyUpdated,
                    );
                    IpcReply {
                        ok: true,
                        message: "Blacklist refreshed and applied.".to_string(),
                    }
                }
                Err(e) => IpcReply {
                    ok: false,
                    message: format!("Refresh failed: {}", e),
                },
            }
        }
        IpcCommand::AiList => {
            let status = if state.is_ai_enabled() { "ON" } else { "OFF" };
            match read_ssd_list("aiserverlist.txt") {
                Ok(list) => IpcReply {
                    ok: true,
                    message: format!("AI Status: {}\n{}", status, list.join("\n")),
                },
                Err(e) => IpcReply {
                    ok: false,
                    message: format!("Failed to read AI list: {}", e),
                },
            }
        }
        IpcCommand::AiEdit { file_path } => match export_to_file("aiserverlist.txt", &file_path) {
            Ok(_) => IpcReply {
                ok: true,
                message: format!("AI server list exported to {}", file_path),
            },
            Err(e) => IpcReply {
                ok: false,
                message: format!("Export failed: {}", e),
            },
        },
        IpcCommand::AiRefresh { file_path } => {
            match import_from_file(&file_path, "aiserverlist.txt") {
                Ok(_) => {
                    state.set_ai_enabled(false);
                    IpcReply { ok: true, message: "AI list refreshed. Access is currently OFF. Please use 'nw aiserverlist on <pass>' to permit access.".to_string() }
                }
                Err(e) => IpcReply {
                    ok: false,
                    message: format!("Refresh failed: {}", e),
                },
            }
        }
        IpcCommand::AiOn { password } => {
            if !state.has_ai_password() {
                return IpcReply { ok: false, message: "Password not set. Please use 'nw aiserverlist passwd' to set initial password.".to_string() };
            }
            let Some(pass) = password else {
                return IpcReply {
                    ok: false,
                    message: "Password required.".to_string(),
                };
            };
            match state.check_ai_password(&pass) {
                Ok(_) => {
                    log_extended(
                        crate::secure_log::Direction::Internal,
                        "nw",
                        "aiserver",
                        "access_on",
                        CauseCode::PolicyUpdated,
                    );
                    IpcReply {
                        ok: true,
                        message: "AI server access PERMITTED.".to_string(),
                    }
                }
                Err(e) => IpcReply {
                    ok: false,
                    message: e,
                },
            }
        }
        IpcCommand::AiOff => {
            state.set_ai_enabled(false);
            log_extended(
                crate::secure_log::Direction::Internal,
                "nw",
                "aiserver",
                "access_off",
                CauseCode::PolicyUpdated,
            );
            IpcReply {
                ok: true,
                message: "AI server access FORBIDDEN.".to_string(),
            }
        }
        IpcCommand::AiPasswd { old_pass, new_pass } => {
            if state.has_ai_password() {
                match state.check_ai_password(&old_pass) {
                    Ok(_) => {
                        state.set_ai_password(&new_pass);
                        IpcReply {
                            ok: true,
                            message: "Password updated successfully.".to_string(),
                        }
                    }
                    Err(e) => IpcReply {
                        ok: false,
                        message: format!("Old password incorrect or account locked: {}", e),
                    },
                }
            } else {
                state.set_ai_password(&new_pass);
                IpcReply {
                    ok: true,
                    message: "Initial password set successfully.".to_string(),
                }
            }
        }
    }
}

fn read_ssd_list(name: &str) -> io::Result<Vec<String>> {
    let path = format!("/media/flux/THPDOC/Develop/TUFF-OS/TUFF-DB/{}", name);
    if !std::path::Path::new(&path).exists() {
        return Ok(Vec::new());
    }
    let mut file = std::fs::File::open(path)?;
    let mut list = Vec::new();
    let mut buf = [0u8; 128];
    use std::io::Read;
    while file.read_exact(&mut buf).is_ok() {
        let s = String::from_utf8_lossy(&buf)
            .trim_matches('\0')
            .trim()
            .to_string();
        if !s.is_empty() {
            list.push(s);
        }
    }
    Ok(list)
}

fn export_to_file(src_name: &str, dest_path: &str) -> io::Result<()> {
    let list = read_ssd_list(src_name)?;
    std::fs::write(dest_path, list.join("\n"))?;
    Ok(())
}

fn import_from_file(src_path: &str, dest_name: &str) -> io::Result<()> {
    let content = std::fs::read_to_string(src_path)?;
    let dest_path = format!("/media/flux/THPDOC/Develop/TUFF-OS/TUFF-DB/{}", dest_name);
    let mut file = std::fs::File::create(dest_path)?;
    use std::io::Write;
    for line in content.lines() {
        let mut buf = [0u8; 128];
        let bytes = line.as_bytes();
        let len = bytes.len().min(128);
        buf[..len].copy_from_slice(&bytes[..len]);
        file.write_all(&buf)?;
    }
    Ok(())
}
