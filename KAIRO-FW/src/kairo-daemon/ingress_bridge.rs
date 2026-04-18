use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use crate::matcher::{Action, MatchInput, TrafficDirection};
use crate::state::KairoState;
use crate::tracker::AgentPidTracker;
use crate::witness_ext::{log_extended, CauseCode};

pub enum IngressDecision {
    Allow,
    DropOnly { note: &'static str },
    DropAndForceOff { note: &'static str },
}

pub fn on_accept_event(
    state: &KairoState,
    tracker: &AgentPidTracker,
    fd: i32,
    remote: SocketAddr,
) -> IngressDecision {
    let ctx = match tracker.flow_ctx(fd) {
        Some(v) => v,
        None => {
            return IngressDecision::DropOnly {
                note: "missing_flow",
            }
        }
    };
    if !tracker.is_agent_pid(ctx.pid) {
        return IngressDecision::Allow;
    }
    let remote_host = remote.ip().to_string();
    let input = MatchInput {
        direction: TrafficDirection::Ingress,
        dest_host: Some(&remote_host),
        dest_ip: Some(remote.ip()),
        dst_is_lan: is_lan_ip(remote.ip()),
        proto: Some("TCP"),
        dest_port: Some(ctx.src_port),
        method: None,
        payload_size: 0,
    };
    let out = state.rules().evaluate(&input);
    if out.action == Some(Action::Kill) {
        state.set_enabled(false);
        tracker.on_close(fd);
        log_extended(
            crate::secure_log::Direction::OutToIn,
            &ctx.pid.to_string(),
            &remote.to_string(),
            "",
            CauseCode::IngressKillMatch,
        );
        return IngressDecision::DropAndForceOff {
            note: "ingress_kill_match",
        };
    }
    if out.action.is_none() {
        return IngressDecision::DropOnly {
            note: "unknown_ingress",
        };
    }
    IngressDecision::Allow
}

pub fn on_recv_event(
    state: &KairoState,
    tracker: &AgentPidTracker,
    fd: i32,
    bytes: usize,
) -> IngressDecision {
    let total = tracker.on_recv(fd, bytes).unwrap_or(0);
    let ctx = match tracker.flow_ctx(fd) {
        Some(v) => v,
        None => {
            return IngressDecision::DropOnly {
                note: "missing_flow",
            }
        }
    };
    if !tracker.is_agent_pid(ctx.pid) {
        return IngressDecision::Allow;
    }
    let dst_ip = ctx.dst.map(|x| x.ip());
    let dst_host = ctx.dst.map(|x| x.ip().to_string()).unwrap_or_default();
    let input = MatchInput {
        direction: TrafficDirection::Ingress,
        dest_host: Some(&dst_host),
        dest_ip: dst_ip,
        dst_is_lan: dst_ip.map(is_lan_ip).unwrap_or(false),
        proto: Some("TCP"),
        dest_port: Some(ctx.src_port),
        method: None,
        payload_size: total as usize,
    };
    let out = state.rules().evaluate(&input);
    if out.action == Some(Action::Kill) {
        state.set_enabled(false);
        tracker.on_close(fd);
        log_extended(
            crate::secure_log::Direction::OutToIn,
            &ctx.pid.to_string(),
            &dst_host,
            "",
            CauseCode::IngressSizeExceeded,
        );
        return IngressDecision::DropAndForceOff {
            note: "ingress_size_exceeded",
        };
    }
    IngressDecision::Allow
}

pub fn check_listen_allowed(
    state: &KairoState,
    tracker: &AgentPidTracker,
    pid: u32,
    port: u16,
    proto: &str,
) -> IngressDecision {
    if !tracker.is_agent_pid(pid) {
        return IngressDecision::Allow;
    }
    let input = MatchInput {
        direction: TrafficDirection::Ingress,
        dest_host: Some("*"),
        dest_ip: Some(IpAddr::from_str("127.0.0.1").unwrap()),
        dst_is_lan: true,
        proto: Some(proto),
        dest_port: Some(port),
        method: None,
        payload_size: 0,
    };
    let out = state.rules().evaluate(&input);
    if out.action == Some(Action::Allow) {
        IngressDecision::Allow
    } else {
        log_extended(
            crate::secure_log::Direction::Internal,
            &pid.to_string(),
            &format!("listen:{}", port),
            "",
            CauseCode::UnauthorizedListen,
        );
        IngressDecision::DropOnly {
            note: "unauthorized_listen",
        }
    }
}

fn is_lan_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 10
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
                || o[0] == 127
        }
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
