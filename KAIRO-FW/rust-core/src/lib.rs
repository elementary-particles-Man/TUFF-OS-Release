//! KAIRO Core Library
pub mod ai_tcp_packet_generated;
pub mod baseline_profile_manager;
pub mod coordination;
pub mod ephemeral_session_generated;
pub mod keygen;
pub mod log_recorder;
pub mod mesh_auditor;
pub mod mesh_trust_calculator;
pub mod packet_parser;
pub mod packet_validator;
pub mod resolvers;
pub mod signature;

// NEW
pub mod bot;
pub mod error;
// 他に必要なら pub mod session_reuse; など

// Placeholder for kairo_core
pub fn placeholder() {
    println!("kairo_core placeholder active");
}
