use std::fs;
use std::path::Path;
use std::process::Command;

// Embed the binaries directly into this installer EXE
const SERVICE_BIN: &[u8] = include_bytes!("../../target/release/kairo-win-service.exe");
const CLI_BIN: &[u8] = include_bytes!("../../target/release/kairo-win-f.exe");
const REFERENCE_MD: &[u8] = include_bytes!("../../KAIRO_WIN_REFERENCE.md");

const INSTALL_DIR: &str = "C:\\Program Files\\THP-lab\\KAIRO-FW";
const SERVICE_NAME: &str = "kairo-win-service";

fn main() {
    println!("=== KAIRO-WIN (Absolute Shield) Installer ===");

    // 1. Create Installation Directory
    let install_path = Path::new(INSTALL_DIR);
    if !install_path.exists() {
        fs::create_dir_all(install_path).expect("Failed to create installation directory");
    }

    // 2. Stop Existing Service
    let _ = Command::new("sc.exe")
        .args(&["stop", SERVICE_NAME])
        .output();

    // 3. Extract Files (Deployment)
    let svc_exe_path = install_path.join("kairo-win-service.exe");
    let cli_exe_path = install_path.join("kairo-win-f.exe");
    let ref_md_path = install_path.join("REFERENCE.md");

    fs::write(&svc_exe_path, SERVICE_BIN).expect("Failed to write service binary");
    fs::write(&cli_exe_path, CLI_BIN).expect("Failed to write CLI binary");
    fs::write(&ref_md_path, REFERENCE_MD).expect("Failed to write reference document");

    // 4. Register and Configure Service
    let _ = Command::new("sc.exe")
        .args(&[
            "create",
            SERVICE_NAME,
            &format!("binPath= \"{}\"", svc_exe_path.to_str().unwrap()),
            "start=",
            "auto",
        ])
        .output();

    // Ensure it's set to auto
    let _ = Command::new("sc.exe")
        .args(&["config", SERVICE_NAME, "start=", "auto"])
        .output();

    // Set Description
    let _ = Command::new("sc.exe")
        .args(&["description", SERVICE_NAME, "KAIRO-WIN - Absolute AI-Proxy Shield (THP-lab)"])
        .output();

    // 5. Start Service
    let _ = Command::new("sc.exe")
        .args(&["start", SERVICE_NAME])
        .output();

    println!("Installation Complete. KAIRO-WIN is now active.");
    println!("Location: {}", INSTALL_DIR);
}
