//! src/kairo-lib/lib.rs

// --- モジュール公開宣言 ---
pub mod comm;
pub mod config;
pub mod mesh_scope_manager;
pub mod packet;
pub mod registry;
pub mod resolvers;
pub mod wau_config;

// --- 構造体・型の再エクスポート ---
pub use comm::{sign_message, Message};
pub use config::AgentConfig;
pub use packet::{AiTcpPacket, PacketSubject, ReplayMetadata};
pub use registry::{
    add_entry, load_registry, register_agent, save_registry, soft_delete_agent, RegistryEntry,
};
