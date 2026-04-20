use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

#[derive(Parser)]
#[command(name = "kairo-fw")]
#[command(about = "KAIRO Firewall Management Tool", version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Network firewall management
    Nw {
        #[command(subcommand)]
        action: NwCommands,
    },
    /// Show firewall status
    Status,
}

#[derive(Subcommand)]
enum NwCommands {
    /// Turn firewall ON (Start service) - Requires root
    On,
    /// Turn firewall OFF (Stop service) - Requires root
    Off,
    /// Manage AI host list (/etc/kairo-fw/ai_hosts.txt)
    Ai {
        #[command(subcommand)]
        action: ListAction,
    },
    /// Manage blacklist (/etc/kairo-fw/blacklist.txt)
    Blacklist {
        #[command(subcommand)]
        action: ListAction,
    },
    /// Reload configuration (restart daemon) - Requires root
    Reload,
}

#[derive(Subcommand)]
enum ListAction {
    /// List all entries
    List,
    /// Add an entry - Requires root
    Add { entry: String },
    /// Remove an entry - Requires root
    Remove { entry: String },
    /// Edit list with system editor - Requires root
    Edit,
}

const CONFIG_DIR: &str = "/etc/kairo-fw";
const AI_HOSTS_FILE: &str = "/etc/kairo-fw/ai_hosts.txt";
const BLACKLIST_FILE: &str = "/etc/kairo-fw/blacklist.txt";

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            let output = Command::new("systemctl")
                .arg("is-active")
                .arg("kairo-fw")
                .output()
                .context("Failed to check systemd status")?;
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("KAIRO-FW Daemon status: {}", status);
        }
        Commands::Nw { action } => match action {
            NwCommands::On => {
                ensure_root()?;
                println!("Starting KAIRO-FW...");
                run_systemctl(&["enable", "kairo-fw"])?;
                run_systemctl(&["start", "kairo-fw"])?;
            }
            NwCommands::Off => {
                ensure_root()?;
                println!("Stopping KAIRO-FW...");
                run_systemctl(&["stop", "kairo-fw"])?;
                run_systemctl(&["disable", "kairo-fw"])?;
            }
            NwCommands::Ai { action } => handle_list(action, AI_HOSTS_FILE)?,
            NwCommands::Blacklist { action } => handle_list(action, BLACKLIST_FILE)?,
            NwCommands::Reload => {
                ensure_root()?;
                println!("Reloading KAIRO-FW configuration...");
                run_systemctl(&["restart", "kairo-fw"])?;
            }
        },
    }

    Ok(())
}

fn ensure_root() -> Result<()> {
    if unsafe { libc::getuid() } != 0 {
        anyhow::bail!("This command requires root privileges. Please run with sudo.");
    }
    Ok(())
}

fn run_systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .args(args)
        .status()
        .context("Failed to execute systemctl")?;
    if !status.success() {
        anyhow::bail!("systemctl command failed.");
    }
    Ok(())
}

fn handle_list(action: ListAction, path: &str) -> Result<()> {
    match action {
        ListAction::List => {
            let entries = read_entries(path)?;
            println!("Entries in {}:", path);
            for (i, entry) in entries.iter().enumerate() {
                println!("  [{:03}] {}", i, entry);
            }
        }
        ListAction::Add { entry } => {
            ensure_root()?;
            ensure_config_dir()?;
            let mut entries = read_entries(path)?;
            let entry = entry.trim().to_string();
            if entries.contains(&entry) {
                println!("Entry '{}' already exists.", entry);
            } else {
                entries.push(entry.clone());
                save_entries(path, &entries)?;
                println!("Added: {}", entry);
            }
        }
        ListAction::Remove { entry } => {
            ensure_root()?;
            let mut entries = read_entries(path)?;
            let entry = entry.trim().to_string();
            if let Some(pos) = entries.iter().position(|x| x == &entry) {
                entries.remove(pos);
                save_entries(path, &entries)?;
                println!("Removed: {}", entry);
            } else {
                println!("Entry '{}' not found.", entry);
            }
        }
        ListAction::Edit => {
            ensure_root()?;
            ensure_config_dir()?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            Command::new(editor)
                .arg(path)
                .status()
                .context("Failed to launch editor")?;
        }
    }
    Ok(())
}

fn ensure_config_dir() -> Result<()> {
    if !Path::new(CONFIG_DIR).exists() {
        fs::create_dir_all(CONFIG_DIR).context("Failed to create config directory")?;
    }
    Ok(())
}

fn read_entries(path: &str) -> Result<Vec<String>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(content.lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect())
}

fn save_entries(path: &str, entries: &[String]) -> Result<()> {
    let content = entries.join("\n") + "\n";
    fs::write(path, content).context(format!("Failed to write to {}", path))?;
    Ok(())
}
