use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

use kairo_lib::packet::Packet;
use log::{info, warn};

/// NickName -> SocketAddr の最小 P ノードレジストリ
static P_NODE_REGISTRY: Lazy<Mutex<HashMap<String, SocketAddr>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// NickName を登録
pub fn register_p_node(nickname: &str, addr: SocketAddr) {
    let mut map = P_NODE_REGISTRY.lock().unwrap();
    map.insert(nickname.to_string(), addr);
    info!("Registered P-node '{}' -> {}", nickname, addr);
}

/// NickName に対応する SocketAddr を返す
pub async fn forward_to_p_node(packet: &Packet) -> Result<Option<SocketAddr>, anyhow::Error> {
    let dest = packet.destination_p_address.trim();

    let map = P_NODE_REGISTRY.lock().unwrap();
    if let Some(addr) = map.get(dest) {
        info!("Forwarding P→P '{}' -> {}", packet.source_p_address, addr);
        return Ok(Some(*addr));
    }

    warn!("forward_to_p_node: Unknown nickname '{}'", dest);
    Ok(None)
}
