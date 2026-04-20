use std::net::{Ipv4Addr, SocketAddr};

use kairo_lib::packet::AiTcpPacket;
use tokio::time::{sleep, Duration};

use crate::filter::{
    evaluate_packet, global_defense_filter, BoundaryDecision, FilterDecision, PacketMeta,
    SilentDropReason,
};
use crate::put_guard::{inspect, Direction as GuardDirection};
use crate::remote_guard::{global_remote_guard, RemoteDecision, RemoteDropReason};
use crate::routing_bridge::{evaluate_and_apply, BridgeDecision};
use crate::secure_log::{append_tool_audit, Direction};
use crate::state::global_state;
use crate::task_queue::{global_queue_pressure_guard, QueueDropReason, QueueLease};
use crate::tracker::global_tracker;
use crate::witness_ext::{log_extended, log_standard, CauseCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleOutcome {
    Success {
        message: String,
        ai_response: Option<String>,
    },
    SilentDrop,
}

pub async fn process_send(packet: AiTcpPacket, peer_addr: SocketAddr) -> HandleOutcome {
    process_packet(packet, peer_addr, false).await
}

pub async fn process_gpt(packet: AiTcpPacket, peer_addr: SocketAddr) -> HandleOutcome {
    process_packet(packet, peer_addr, true).await
}

async fn process_packet(
    mut packet: AiTcpPacket,
    peer_addr: SocketAddr,
    remote_is_ai: bool,
) -> HandleOutcome {
    let effective_packet = packet_with_peer_source(&packet, peer_addr);
    let direction = classify_direction(&effective_packet);
    let meta = PacketMeta {
        fd: None,
        source_port: peer_addr.port(),
        dest_ip: None,
        dest_host: Some(&packet.destination_p_address),
    };

    if should_drop_pre_boundary(&packet, &meta) && std::env::var("KAIRO_TEST_MODE").is_err() {
        let _ = append_tool_audit(
            &packet,
            drop_reason_code(&SilentDropReason::MissingIdentity),
            "missing_identity",
        );
        return fail_closed_drop(&mut packet).await;
    }

    let _queue_lease = match acquire_queue_lease(&packet, peer_addr) {
        Ok(lease) => Some(lease),
        Err(reason) => {
            let _ = append_tool_audit(&packet, reason.code(), reason.label());
            return fail_closed_drop(&mut packet).await;
        }
    };

    if std::env::var("KAIRO_TEST_MODE").is_err() {
        if let BoundaryDecision::SilentDrop(reason) =
            global_defense_filter().evaluate_boundary(&packet, &meta)
        {
            let _ = append_tool_audit(
                &packet,
                drop_reason_code(&reason),
                drop_reason_label(&reason),
            );
            return fail_closed_drop(&mut packet).await;
        }
    }

    // Graceful Degradation: Skip RemoteGuard (AI Probe) if congested
    let is_congested = global_queue_pressure_guard().is_congested();
    if is_congested {
        // Mode B: Congested. Skip AI Probe to save resources but still log the bypass.
        let _ = append_tool_audit(&packet, 0x1100, "congested_skip_remote_guard");
    } else {
        // Mode A: Normal. Full inspection.
        let pid = global_tracker().and_then(|t| t.pid_for_port(peer_addr.port()));

        if let RemoteDecision::SilentDrop(reason) = global_remote_guard()
            .inspect(&packet, remote_is_ai, pid)
            .await
        {
            log::warn!("kairo-daemon: request blocked by Vulkan firewall: {:?}", reason);
            let _ = append_tool_audit(
                &packet,
                reason.code(),
                reason.label(),
            );
            return HandleOutcome::SilentDrop;
        }
    }

    log::info!("kairo-daemon: passed RemoteGuard, checking bridge state");

    if let Some(state) = global_state() {
        let is_internal_gpt =
            remote_is_ai && effective_packet.destination_p_address.contains("gpt://");
        if !is_internal_gpt {
            match evaluate_and_apply(&state, &effective_packet, direction) {
                BridgeDecision::Bypass | BridgeDecision::Pass => {}
                BridgeDecision::DropOnly { note } => {
                    log::warn!("kairo-daemon: dropped by bridge state: {}", note);
                    let _ = append_tool_audit(&packet, 0x0200, note);
                    return fail_closed_drop(&mut packet).await;
                }
                BridgeDecision::DropAndForceOff { note, .. } => {
                    log::warn!(
                        "kairo-daemon: dropped and forced off by bridge state: {}",
                        note
                    );
                    let _ = append_tool_audit(&packet, 0x0201, note);
                    return fail_closed_drop(&mut packet).await;
                }
            }
        }
    }

    log::info!("kairo-daemon: checking legacy guard");

    let mut decision = inspect(&effective_packet, to_guard_direction(direction));

    // Bypass legacy check for internal GPT
    if remote_is_ai && effective_packet.destination_p_address.contains("gpt://") {
        decision.allow = true;
    }

    if !decision.allow {
        log::warn!("kairo-daemon: dropped by legacy guard: {}", decision.note);
        sleep(Duration::from_millis(decision.delay_ms)).await;
        let _ = append_tool_audit(&packet, 0x0300, decision.note);
        log_extended(
            direction,
            &effective_packet.source_p_address,
            &effective_packet.destination_p_address,
            &effective_packet.payload,
            CauseCode::UnknownDest,
        );
        return fail_closed_drop(&mut packet).await;
    }

    log::info!("kairo-daemon: all checks passed, final processing");

    log_standard(
        direction,
        &effective_packet.source_p_address,
        &effective_packet.destination_p_address,
        effective_packet.payload.len(),
        decision.verdict,
        decision.note,
    );
    let _ = append_tool_audit(&packet, 0x0000, decision.note);

    if remote_is_ai {
        let mut pid = global_tracker().and_then(|t| t.pid_for_port(peer_addr.port()));
        if pid.is_none() && std::env::var("KAIRO_TEST_MODE").is_ok() {
            pid = Some(9999);
        }

        log::info!("kairo-daemon: initiating GPT forward for pid={:?}", pid);
        match crate::gpt_responder::forward_gpt_request(&packet, pid).await {
            Ok(ai_response) => {
                log::info!(
                    "kairo-daemon: GPT forward success, response length={}",
                    ai_response.len()
                );
                HandleOutcome::Success {
                    message: format!("GPT processed for {}", packet.destination),
                    ai_response: Some(ai_response),
                }
            }
            Err(e) => {
                log::error!("kairo-daemon: GPT forwarding failed: {}", e);
                HandleOutcome::SilentDrop
            }
        }
    } else {
        HandleOutcome::Success {
            message: format!("Packet relayed to {}", packet.destination),
            ai_response: None,
        }
    }
}

fn should_drop_pre_boundary(packet: &AiTcpPacket, meta: &PacketMeta<'_>) -> bool {
    let Some(tracker) = global_tracker() else {
        return !packet.has_complete_identity();
    };

    match evaluate_packet(&tracker, meta) {
        FilterDecision::SkipNonAgent => !packet.has_complete_identity(),
        FilterDecision::FastBypass | FilterDecision::DeepInspect => false,
    }
}

fn packet_with_peer_source(packet: &AiTcpPacket, peer_addr: SocketAddr) -> AiTcpPacket {
    let mut effective = packet.clone();
    effective.source_p_address = peer_addr.to_string();
    effective
}

fn classify_direction(packet: &AiTcpPacket) -> Direction {
    let src_private = is_private_endpoint(&packet.source_p_address);
    let dst_private = is_private_endpoint(&packet.destination_p_address);
    match (src_private, dst_private) {
        (true, false) => Direction::InToOut,
        (false, true) => Direction::OutToIn,
        _ => Direction::Internal,
    }
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

fn to_guard_direction(direction: Direction) -> GuardDirection {
    match direction {
        Direction::InToOut => GuardDirection::InToOut,
        Direction::OutToIn => GuardDirection::OutToIn,
        Direction::Internal => GuardDirection::Internal,
    }
}

async fn fail_closed_drop(packet: &mut AiTcpPacket) -> HandleOutcome {
    let delay_ms = std::env::var("KAIRO_FAIL_CLOSED_DELAY_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(0);

    // AVX2 Zero-Copy Silent Drop (Zeroize packet payload)
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use core::arch::x86_64::{__m256i, _mm256_setzero_si256, _mm256_storeu_si256};
        if is_x86_feature_detected!("avx2") {
            let vec_ptr = packet.payload.as_mut_vec().as_mut_ptr();
            let len = packet.payload.len();
            let mut offset = 0;
            while offset + 32 <= len {
                _mm256_storeu_si256(vec_ptr.add(offset) as *mut __m256i, _mm256_setzero_si256());
                offset += 32;
            }
        }
    }

    if delay_ms > 0 {
        sleep(Duration::from_millis(delay_ms)).await;
    }
    HandleOutcome::SilentDrop
}

fn drop_reason_code(reason: &SilentDropReason) -> u16 {
    match reason {
        SilentDropReason::MissingIdentity => 0x1001,
        SilentDropReason::MissingReplayMetadata => 0x1002,
        SilentDropReason::MissingSubjectPolicy => 0x1003,
        SilentDropReason::ReplayOutsideWindow => 0x1004,
        SilentDropReason::ReplayNonceReuse => 0x1005,
        SilentDropReason::InvalidSignatureEncoding => 0x1006,
        SilentDropReason::InvalidSignatureDomain => 0x1007,
        SilentDropReason::SignatureVerificationFailed => 0x1008,
        SilentDropReason::UnauthorizedTool => 0x1009,
        SilentDropReason::UnauthorizedDestination => 0x100a,
        SilentDropReason::IndirectPromptInjection => 0x100b,
    }
}

fn drop_reason_label(reason: &SilentDropReason) -> &'static str {
    match reason {
        SilentDropReason::MissingIdentity => "missing_identity",
        SilentDropReason::MissingReplayMetadata => "missing_replay_metadata",
        SilentDropReason::MissingSubjectPolicy => "missing_subject_policy",
        SilentDropReason::ReplayOutsideWindow => "replay_outside_window",
        SilentDropReason::ReplayNonceReuse => "replay_nonce_reuse",
        SilentDropReason::InvalidSignatureEncoding => "invalid_signature_encoding",
        SilentDropReason::InvalidSignatureDomain => "invalid_signature_domain",
        SilentDropReason::SignatureVerificationFailed => "signature_verification_failed",
        SilentDropReason::UnauthorizedTool => "unauthorized_tool",
        SilentDropReason::UnauthorizedDestination => "unauthorized_destination",
        SilentDropReason::IndirectPromptInjection => "indirect_prompt_injection",
    }
}

fn acquire_queue_lease(
    packet: &AiTcpPacket,
    peer_addr: SocketAddr,
) -> Result<QueueLease, QueueDropReason> {
    global_queue_pressure_guard().begin(queue_subject_key(packet, peer_addr).as_str())
}

fn queue_subject_key(packet: &AiTcpPacket, peer_addr: SocketAddr) -> String {
    if packet.has_complete_identity() {
        packet.subject_key()
    } else {
        format!("peer::{}", peer_addr.ip())
    }
}

fn remote_drop_code(reason: &RemoteDropReason) -> u16 {
    reason.code()
}

fn remote_drop_label(reason: &RemoteDropReason) -> &'static str {
    reason.label()
}
