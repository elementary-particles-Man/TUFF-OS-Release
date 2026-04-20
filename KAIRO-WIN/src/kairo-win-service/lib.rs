mod clear_mini_state;
mod config;
mod cse;
pub mod filter;
pub mod gpt_responder;
mod handler;
mod ingress_bridge;
mod ip_classifier;
mod ipc;
mod khp;
mod matcher;
mod p_registry;
mod put_guard;
pub mod remote_guard;
mod routing_bridge;
pub mod secure_log;
mod state;
mod task_queue;
pub mod tracker;
mod vulkan_gpu;
pub mod witness_ext;

mod api {
    pub mod controller;
}


use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::http::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{interval, MissedTickBehavior};

use config::{load_acl_from_path, DEFAULT_ACL_PATH};
use filter::configure_global_defense_filter_from_env;
use handler::{process_gpt, process_send, HandleOutcome};
use matcher::RuleSet;
use remote_guard::configure_global_remote_guard;
use simplelog::{
    ColorChoice, CombinedLogger, Config as LogConfig, LevelFilter, TermLogger, TerminalMode,
};
use state::KairoState;
use task_queue::configure_global_queue_pressure_guard_from_env;
use tracker::{set_global_tracker, AgentPidTracker};
use vulkan_gpu::global_backend;

const MAX_HTTP_HEADER_BYTES: usize = 32 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_VULKAN_HEARTBEAT_SECS: u64 = 60;

pub async fn run_embedded_daemon() -> Result<()> {
    init_logger();

    let runtime_state = Arc::new(KairoState::new(RuleSet::empty()));
    let tracker = Arc::new(AgentPidTracker::new());
    set_global_tracker(tracker);
    state::set_global_state(runtime_state.clone());

    if let Some(tracker) = crate::tracker::global_tracker() {
        bootstrap_tracker_from_env(&tracker)?;
    }
    if configure_global_defense_filter_from_env()
        .map_err(anyhow::Error::msg)?
        .is_some()
    {
        log::info!("configured KAIRO defense filter from environment");
    }
    let _remote_guard = configure_global_remote_guard().map_err(anyhow::Error::msg)?;
    log::info!("configured KAIRO-FW remote guard from files and env");
    configure_global_queue_pressure_guard_from_env().map_err(anyhow::Error::msg)?;
    log::info!("configured KAIRO queue pressure guard");

    match load_acl_from_path(std::path::Path::new(DEFAULT_ACL_PATH)) {
        Ok(loaded) => {
            runtime_state.swap_rules(loaded.rules);
            runtime_state.mark_acl_status(true, loaded.allow_count);
            log::info!("loaded ACL from {}", DEFAULT_ACL_PATH);
        }
        Err(e) => {
            runtime_state.mark_acl_status(false, 0);
            log::error!("failed to load ACL: {}", e);
        }
    }

    if let Err(e) = ipc::spawn_ipc_listener(runtime_state.clone()).await {
        log::error!("failed to start IPC listener: {}", e);
    }
    spawn_vulkan_metrics_heartbeat();

    let addr = daemon_bind_addr()?;
    log::info!("kairo-daemon: listening on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = serve_connection(stream, peer_addr).await {
                log::warn!("kairo-daemon: connection {} failed: {}", peer_addr, err);
            }
        });
    }
}

async fn serve_connection(mut stream: tokio::net::TcpStream, peer_addr: SocketAddr) -> Result<()> {
    println!("TRACE: serve_connection start for {}", peer_addr);
    log::debug!("kairo-daemon: serving connection from {}", peer_addr);
    match read_http_request(&mut stream).await {
        Ok(request) => match dispatch_request(&mut stream, peer_addr, request).await? {
            ConnectionDisposition::Completed => Ok(()),
            ConnectionDisposition::SilentDrop => {
                let _ = stream.set_linger(Some(Duration::ZERO));
                Ok(())
            }
        },
        Err(()) => {
            let _ = apply_fail_closed_delay().await;
            let _ = stream.set_linger(Some(Duration::ZERO));
            Ok(())
        }
    }
}

#[derive(Debug)]
enum ConnectionDisposition {
    Completed,
    SilentDrop,
}

#[derive(Debug)]
struct ParsedRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

async fn dispatch_request(
    stream: &mut tokio::net::TcpStream,
    peer_addr: SocketAddr,
    request: ParsedRequest,
) -> Result<ConnectionDisposition> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => {
            write_response(
                stream,
                StatusCode::OK,
                "text/plain; charset=utf-8",
                b"KAIRO Daemon Online",
            )
            .await?;
            Ok(ConnectionDisposition::Completed)
        }
        #[cfg(debug_assertions)]
        ("GET", "/_internal_debug/dump") => {
            use crate::clear_mini_state::CLEAR_MINI;

            log::warn!("Executing debug dump API. This MUST NOT appear in release builds.");
            let snapshot = CLEAR_MINI.lock().unwrap().dump_witness_snapshot();
            let body = serde_json::to_vec(&snapshot)?;
            write_response(stream, StatusCode::OK, "application/json", &body).await?;
            Ok(ConnectionDisposition::Completed)
        }
        ("POST", "/send") => handle_packet_request(stream, peer_addr, &request.body, false).await,
        ("POST", "/gpt") => handle_packet_request(stream, peer_addr, &request.body, true).await,
        (_, "/send" | "/gpt" | "/add_task") => Ok(ConnectionDisposition::SilentDrop),
        _ => {
            write_response(
                stream,
                StatusCode::NOT_FOUND,
                "text/plain; charset=utf-8",
                b"not found",
            )
            .await?;
            Ok(ConnectionDisposition::Completed)
        }
    }
}

async fn handle_packet_request(
    stream: &mut tokio::net::TcpStream,
    peer_addr: SocketAddr,
    body: &[u8],
    is_gpt: bool,
) -> Result<ConnectionDisposition> {
    let packet: kairo_lib::packet::AiTcpPacket = match serde_json::from_slice(body) {
        Ok(packet) => packet,
        Err(e) => {
            log::warn!(
                "kairo-daemon: failed to parse JSON from {}: {}",
                peer_addr,
                e
            );
            return Ok(ConnectionDisposition::SilentDrop);
        }
    };

    log::debug!(
        "kairo-daemon: handling {} request from {} for {}",
        if is_gpt { "GPT" } else { "Send" },
        peer_addr,
        packet.destination
    );

    let outcome = if is_gpt {
        process_gpt(packet, peer_addr).await
    } else {
        process_send(packet, peer_addr).await
    };

    match outcome {
        HandleOutcome::Success {
            message,
            ai_response,
        } => {
            if let Some(resp) = ai_response {
                write_response(stream, StatusCode::OK, "application/json", resp.as_bytes()).await?;
            } else {
                let body = serde_json::to_vec(&message)?;
                write_response(stream, StatusCode::OK, "application/json", &body).await?;
            }
            Ok(ConnectionDisposition::Completed)
        }
        HandleOutcome::SilentDrop => Ok(ConnectionDisposition::SilentDrop),
    }
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<ParsedRequest, ()> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(offset) = find_header_end(&buffer) {
            break offset;
        }
        if buffer.len() >= MAX_HTTP_HEADER_BYTES {
            log::warn!("kairo-daemon: HTTP header too large");
            return Err(());
        }
        let read = stream.read(&mut chunk).await.map_err(|_| ())?;
        if read == 0 {
            if !buffer.is_empty() {
                log::warn!("kairo-daemon: connection closed before header end found");
            }
            return Err(());
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let headers = std::str::from_utf8(&buffer[..header_end]).map_err(|_| {
        log::warn!("kairo-daemon: HTTP headers are not valid UTF-8");
        ()
    })?;
    let mut lines = headers.split("\r\n");
    let request_line = lines.next().ok_or_else(|| {
        log::warn!("kairo-daemon: empty HTTP request line");
        ()
    })?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| {
            log::warn!("kairo-daemon: missing HTTP method");
            ()
        })?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| {
            log::warn!("kairo-daemon: missing HTTP path");
            ()
        })?
        .to_string();
    if request_parts.next().is_none() {
        log::warn!("kairo-daemon: missing HTTP version");
        return Err(());
    }

    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            log::warn!("kairo-daemon: invalid HTTP header line: {}", line);
            return Err(());
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse::<usize>().map_err(|e| {
                log::warn!("kairo-daemon: invalid content-length: {}", e);
                ()
            })?;
        }
    }
    if content_length > MAX_HTTP_BODY_BYTES {
        log::warn!("kairo-daemon: HTTP body too large: {}", content_length);
        return Err(());
    }

    let total_len = header_end + content_length;
    while buffer.len() < total_len {
        let read = stream.read(&mut chunk).await.map_err(|_| {
            log::warn!("kairo-daemon: failed to read HTTP body");
            ()
        })?;
        if read == 0 {
            log::warn!("kairo-daemon: connection closed before full body received");
            return Err(());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    Ok(ParsedRequest {
        method,
        path,
        body: buffer[header_end..total_len].to_vec(),
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    status: StatusCode,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let status_line = match status {
        StatusCode::OK => "200 OK",
        StatusCode::NOT_FOUND => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_line,
        content_type,
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.shutdown().await?;
    Ok(())
}

fn init_logger() {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|raw| raw.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);

    let _ = CombinedLogger::init(vec![TermLogger::new(
        filter,
        LogConfig::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )]);
}

fn daemon_bind_addr() -> Result<SocketAddr> {
    match std::env::var("KAIRO_DAEMON_ADDR") {
        Ok(raw) => raw
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid KAIRO_DAEMON_ADDR: {}", raw)),
        Err(_) => Ok(SocketAddr::from(([127, 0, 0, 1], 8080))),
    }
}

fn spawn_vulkan_metrics_heartbeat() {
    let Some(period) = vulkan_metrics_heartbeat_period() else {
        log::info!("kairo-daemon: vulkan metrics heartbeat disabled");
        return;
    };
    tokio::spawn(async move {
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let counters = global_backend().debug_counters();
            let has_activity = counters.submissions > 0
                || counters.vulkan_completions > 0
                || counters.cpu_fallbacks > 0
                || counters.zeroize_requests > 0;
            if !has_activity {
                continue;
            }
            log::info!(
                "kairo-daemon: vulkan heartbeat submissions={} completions={} cpu_fallbacks={} timeouts={} zeroize_requests={} zeroize_immediate={} bytes_up={} bytes_down={} packet=[{}] maintenance=[{}] audit=[{}] bulk=[{}]",
                counters.submissions,
                counters.vulkan_completions,
                counters.cpu_fallbacks,
                counters.timeouts,
                counters.zeroize_requests,
                counters.zeroize_immediate,
                counters.bytes_uploaded,
                counters.bytes_downloaded,
                counters.packet_preclassification.summary(),
                counters.maintenance_hashing.summary(),
                counters.audit_scan.summary(),
                counters.bulk_prefilter.summary(),
            );
        }
    });
}

fn vulkan_metrics_heartbeat_period() -> Option<Duration> {
    if env_flag("KAIRO_VULKAN_HEARTBEAT_DISABLE").unwrap_or(false) {
        return None;
    }
    let seconds = env_nonempty("KAIRO_VULKAN_HEARTBEAT_SECS")
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(DEFAULT_VULKAN_HEARTBEAT_SECS);
    (seconds > 0).then_some(Duration::from_secs(seconds))
}

fn bootstrap_tracker_from_env(tracker: &AgentPidTracker) -> Result<()> {
    let Some(pid_raw) = env_nonempty("KAIRO_DEFENSE_AGENT_PID") else {
        return Ok(());
    };
    let pid = pid_raw
        .parse::<u32>()
        .with_context(|| format!("invalid KAIRO_DEFENSE_AGENT_PID: {}", pid_raw))?;
    tracker.register_agent_pid(pid);

    let mut mapped_ports = Vec::new();
    for port_raw in env_csv("KAIRO_DEFENSE_AGENT_PORTS") {
        let port = port_raw
            .parse::<u16>()
            .with_context(|| format!("invalid KAIRO_DEFENSE_AGENT_PORTS entry: {}", port_raw))?;
        tracker.map_source_port(port, pid);
        mapped_ports.push(port);
    }

    if mapped_ports.is_empty() {
        log::warn!("agent pid {} registered without source port mappings", pid);
    } else {
        log::info!("registered agent pid {} on ports {:?}", pid, mapped_ports);
    }
    Ok(())
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn env_flag(name: &str) -> Option<bool> {
    env_nonempty(name).map(|raw| matches!(raw.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn env_csv(name: &str) -> Vec<String> {
    env_nonempty(name)
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkan_heartbeat_defaults_to_safe_period() {
        std::env::remove_var("KAIRO_VULKAN_HEARTBEAT_DISABLE");
        std::env::remove_var("KAIRO_VULKAN_HEARTBEAT_SECS");
        assert_eq!(
            vulkan_metrics_heartbeat_period(),
            Some(Duration::from_secs(DEFAULT_VULKAN_HEARTBEAT_SECS))
        );
    }

    #[test]
    fn vulkan_heartbeat_can_be_disabled_by_env() {
        std::env::set_var("KAIRO_VULKAN_HEARTBEAT_DISABLE", "1");
        assert_eq!(vulkan_metrics_heartbeat_period(), None);
        std::env::remove_var("KAIRO_VULKAN_HEARTBEAT_DISABLE");
    }
}

async fn apply_fail_closed_delay() -> Result<()> {
    let delay_ms = std::env::var("KAIRO_FAIL_CLOSED_DELAY_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(0);
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
    Ok(())
}
