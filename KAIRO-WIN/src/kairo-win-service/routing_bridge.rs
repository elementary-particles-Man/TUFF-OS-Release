use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::sync::Arc;

use kairo_lib::packet::AiTcpPacket;

use crate::matcher::{MatchInput, TrafficDirection};
use crate::secure_log::Direction;
use crate::state::{KairoState, UnknownMode};
use crate::witness_ext::{log_extended, log_standard, CauseCode};

pub enum BridgeDecision {
    Bypass,
    Pass,
    DropOnly {
        note: &'static str,
    },
    DropAndForceOff {
        note: &'static str,
        cause: CauseCode,
    },
}

pub fn evaluate_and_apply(
    state: &Arc<KairoState>,
    packet: &AiTcpPacket,
    direction: Direction,
) -> BridgeDecision {
    let normalized_host = normalize_host(&packet.destination_p_address);
    let dest_ip = parse_ip(&packet.destination_p_address);
    let dest_port = parse_port(&packet.destination_p_address);
    let proto = infer_proto(dest_port, packet.payload_type.as_str());
    let method = detect_http_method(&packet.payload);
    let dst_is_lan = is_private_endpoint(&packet.destination_p_address);
    let input = MatchInput {
        direction: TrafficDirection::Egress,
        dest_host: Some(normalized_host.as_str()),
        dest_ip,
        dst_is_lan,
        proto: proto.as_deref(),
        dest_port,
        method,
        payload_size: packet.payload.len(),
    };
    let res = state.rules().evaluate(&input);

    if res.action.is_some() && res.reason == "kill_match" {
        state.set_enabled(false);
        log_extended(
            direction,
            &packet.source_p_address,
            &packet.destination_p_address,
            &packet.payload,
            CauseCode::KillMatch,
        );
        return BridgeDecision::DropAndForceOff {
            note: "kill_match",
            cause: CauseCode::KillMatch,
        };
    }
    if res.action.is_some() && res.reason == "bypass_match" {
        return BridgeDecision::Bypass;
    }

    if res.action.is_none() && !dst_is_lan {
        match state.unknown_mode() {
            UnknownMode::DropAndForceOff => {
                state.set_enabled(false);
                log_extended(
                    direction,
                    &packet.source_p_address,
                    &packet.destination_p_address,
                    &packet.payload,
                    CauseCode::UnknownDest,
                );
                BridgeDecision::DropAndForceOff {
                    note: "unknown_dest",
                    cause: CauseCode::UnknownDest,
                }
            }
            UnknownMode::DropOnly => {
                log_standard(
                    direction,
                    &packet.source_p_address,
                    &packet.destination_p_address,
                    packet.payload.len(),
                    "drop_only",
                    "unknown_dest",
                );
                BridgeDecision::DropOnly {
                    note: "unknown_dest_drop_only",
                }
            }
        }
    } else {
        BridgeDecision::Pass
    }
}

fn parse_ip(endpoint: &str) -> Option<IpAddr> {
    let host = endpoint.rsplit("://").next().unwrap_or(endpoint);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    IpAddr::from_str(host).ok()
}

fn parse_port(endpoint: &str) -> Option<u16> {
    let host = endpoint.rsplit("://").next().unwrap_or(endpoint);
    let host = host.split('/').next().unwrap_or(host);
    let (_, port) = host.rsplit_once(':')?;
    port.parse::<u16>().ok()
}

fn infer_proto(dest_port: Option<u16>, payload_type: &str) -> Option<String> {
    if payload_type.to_ascii_uppercase().contains("SMB") {
        return Some("SMB".to_string());
    }
    if payload_type.to_ascii_uppercase().contains("SMTP") {
        return Some("SMTP".to_string());
    }
    match dest_port {
        Some(25 | 465 | 587) => Some("SMTP".to_string()),
        Some(445) => Some("SMB".to_string()),
        Some(443) => Some("HTTPS".to_string()),
        Some(80 | 8080) => Some("HTTP".to_string()),
        Some(53) => Some("DNS".to_string()),
        Some(123) => Some("NTP".to_string()),
        _ => None,
    }
}

fn detect_http_method(payload: &str) -> Option<&str> {
    let head = payload.trim_start();
    for m in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
        if head.starts_with(m) {
            return Some(m);
        }
    }
    None
}

fn is_private_endpoint(endpoint: &str) -> bool {
    if endpoint.eq_ignore_ascii_case("localhost") || endpoint.ends_with(".local") {
        return true;
    }
    let host = endpoint.rsplit("://").next().unwrap_or(endpoint);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return false;
    };
    let octets = ip.octets();
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || octets[0] == 127
}

fn normalize_host(endpoint: &str) -> String {
    endpoint
        .rsplit("://")
        .next()
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or(endpoint)
        .split(':')
        .next()
        .unwrap_or(endpoint)
        .to_ascii_lowercase()
}
