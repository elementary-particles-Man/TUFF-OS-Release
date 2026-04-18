use dashmap::DashMap;
use kairo_lib::packet::AiTcpPacket;
use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const WINDOW_SECS: u64 = 10;
const MAX_PUTS_PER_WINDOW: usize = 20;
const MAX_PAYLOAD_BYTES: usize = 4096;
const MAX_EXTERNAL_URLS_PER_WINDOW: usize = 5;

static RATE: Lazy<DashMap<String, VecDeque<Instant>>> = Lazy::new(DashMap::new);
static EXTERNAL_URL_RATE: Lazy<DashMap<String, VecDeque<Instant>>> = Lazy::new(DashMap::new);

pub struct Decision {
    pub allow: bool,
    pub delay_ms: u64,
    pub verdict: &'static str,
    pub note: &'static str,
}

#[derive(Clone, Copy)]
pub enum Direction {
    InToOut,
    OutToIn,
    Internal,
}

pub fn inspect(packet: &AiTcpPacket, direction: Direction) -> Decision {
    if packet.payload.len() > MAX_PAYLOAD_BYTES {
        return deny_with_delay(direction, "oversized_payload");
    }

    if contains_billing_indicator(&packet.payload) {
        return deny_with_delay(direction, "billing_risk_pattern");
    }

    let source = packet.source_p_address.as_str();
    if too_many_requests(source, &RATE, MAX_PUTS_PER_WINDOW) {
        return deny_with_delay(direction, "rate_limit_exceeded");
    }

    if matches!(direction, Direction::InToOut) && payload_has_external_url(&packet.payload) {
        if too_many_requests(source, &EXTERNAL_URL_RATE, MAX_EXTERNAL_URLS_PER_WINDOW) {
            return deny_with_delay(direction, "external_put_burst");
        }
    }

    Decision {
        allow: true,
        delay_ms: 0,
        verdict: "accepted",
        note: "policy_pass",
    }
}

fn deny_with_delay(direction: Direction, note: &'static str) -> Decision {
    let delay_ms = match direction {
        Direction::InToOut => 4_000,
        Direction::OutToIn => 3_000,
        Direction::Internal => 800,
    };
    Decision {
        allow: false,
        delay_ms,
        verdict: "delayed_reject",
        note,
    }
}

fn too_many_requests(key: &str, table: &DashMap<String, VecDeque<Instant>>, limit: usize) -> bool {
    let now = Instant::now();
    let mut queue = table.entry(key.to_string()).or_default();
    queue.push_back(now);
    let win = Duration::from_secs(WINDOW_SECS);
    while let Some(front) = queue.front().copied() {
        if now.duration_since(front) > win {
            queue.pop_front();
        } else {
            break;
        }
    }
    queue.len() > limit
}

fn payload_has_external_url(payload: &str) -> bool {
    payload.contains("http://") || payload.contains("https://")
}

fn contains_billing_indicator(payload: &str) -> bool {
    let p = payload.to_ascii_lowercase();
    let indicators = [
        "billing",
        "charge",
        "payment",
        "invoice",
        "subscription",
        "openai.com/v1",
        "anthropic.com",
        "gemini",
        "api_key",
        "bearer ",
    ];
    indicators.iter().any(|needle| p.contains(needle))
}
