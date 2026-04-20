use crate::secure_log::{append_packet_event, Direction, PacketEvent};
use log::{error, info};

/// GPT応答を CSE + 3J で暗号チャンク保存する
pub async fn log_gpt_response(
    response: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let result = append_packet_event(PacketEvent {
        direction: Direction::Internal,
        source: "gpt://responder",
        destination: "kairo://secure-log",
        payload_len: response.len(),
        verdict: "logged",
        note: "gpt_response",
    });
    match result {
        Ok(()) => {
            info!("✅ GPT response logged to CSE chunk");
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to write secure GPT log entry: {}", e);
            Err(e.into())
        }
    }
}
