use anyhow::Result;
use kairo_lib::packet::AiTcpPacket;
use log::info;
use reqwest::Client;
use std::time::Duration;

/// AIのプロンプト介入をバイパス（倫理実装削除済み）
pub async fn apply_active_disruption(_packet: &mut AiTcpPacket, _pid: u32) -> Result<bool> {
    Ok(false)
}

/// 実際にAIへリクエストを転送し、レスポンスを取得する
pub async fn forward_gpt_request(packet: &AiTcpPacket, _pid: Option<u32>) -> Result<String> {
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

    // destination が URL でない場合のフォールバック (gpt://main など)
    let url = if packet.destination.starts_with("http") {
        packet.destination.clone()
    } else {
        // テスト用：環境変数から取得するか、デフォルトのモックサーバーへ
        std::env::var("KAIRO_MOCK_AI_URL")
            .unwrap_or_else(|_| "http://localhost:5000/v1/chat/completions".to_string())
    };

    info!("kairo-daemon: forwarding GPT request to {}", url);

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(packet.payload.clone())
        .send()
        .await?;

    let body = resp.text().await?;

    Ok(body)
}

/// レスポンス監視をバイパス（倫理実装削除済み）
pub fn monitor_resonance(_pid: u32, _response_body: &str) {
    // No-op
}
