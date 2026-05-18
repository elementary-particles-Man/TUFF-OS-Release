use std::env;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use fips204::traits::{KeyGen, SerDes, Signer};
use kairo_lib::packet::AiTcpPacket;
use reqwest::Client;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tuff_core::pq::signatures::{PqSignatureBlob, PQ_SIGNATURE_PLACEHOLDER_LEN};
use tuff_core::pq::{self, PqDomain, PqSignatureAlgorithm};
use url::Url;

const DEFAULT_SEED_HEX: &str = "4141414141414141414141414141414141414141414141414141414141414141";
const AUTO_ENDPOINTS: &[&str] = &[
    "http://127.0.0.1:8080/send",
    "http://127.0.0.1:18080/send",
    "http://127.0.0.1:3000/send",
    "http://127.0.0.1:3001/send",
    "http://127.0.0.1:9000/send",
    "http://localhost:8080/send",
    "http://localhost:18080/send",
];

#[derive(Debug, Parser)]
#[command(name = "kairo-striker")]
#[command(about = "KAIRO NW attack harness")]
pub struct Cli {
    #[arg(
        long,
        default_value = "auto",
        help = "Comma-separated endpoint list or 'auto'"
    )]
    endpoint: String,
    #[arg(long, default_value = "agent-alpha")]
    agent_id: String,
    #[arg(long, default_value = "session-01")]
    session_id: String,
    #[arg(long, default_value = "10.0.0.2:51000")]
    source_p_address: String,
    #[arg(long, default_value = "https://api.openai.com:443")]
    destination: String,
    #[arg(long, default_value = "fetch_context")]
    tool_name: String,
    #[arg(long, default_value_t = 250)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 5)]
    rounds: usize,
    #[arg(long, default_value_t = 32)]
    concurrency: usize,
    #[arg(long, default_value_t = 64)]
    burst_count: usize,
    #[arg(long, default_value_t = 3)]
    slow_drip_bytes: usize,
    #[arg(long, default_value = DEFAULT_SEED_HEX)]
    seed_hex: String,
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Debug, Subcommand)]
enum Mode {
    Gauntlet {
        #[arg(long, default_value = "{\"path\":\"/kb/index\"}")]
        payload: String,
        #[arg(long, default_value = "https://203.0.113.10:443")]
        rogue_destination: String,
    },
    Burst {
        #[arg(long, default_value_t = 64)]
        count: usize,
        #[arg(long, default_value_t = 32)]
        concurrency: usize,
        #[arg(long, default_value = "{\"path\":\"/kb/index\"}")]
        payload: String,
    },
    Replay {
        #[arg(long, default_value = "{\"path\":\"/kb/index\"}")]
        payload: String,
    },
    RogueDst {
        #[arg(long, default_value = "https://203.0.113.10:443")]
        rogue_destination: String,
        #[arg(long, default_value = "{\"path\":\"/admin/export\"}")]
        payload: String,
    },
    SignatureStorm {
        #[arg(long, default_value_t = 48)]
        count: usize,
    },
    SlowDrip {
        #[arg(long, default_value_t = 3)]
        bytes: usize,
    },
    Probe,
}

#[derive(Debug, Clone)]
struct PacketTemplate {
    agent_id: String,
    session_id: String,
    source_p_address: String,
    destination: String,
    tool_name: String,
    seed: [u8; 32],
}

#[derive(Debug)]
enum RequestOutcome {
    Timeout,
    Http { status: u16, body: String },
    TransportError,
    Skipped,
}

#[derive(Debug, Default)]
struct ScenarioSummary {
    mode: String,
    sent: usize,
    timeouts: usize,
    status_200: usize,
    status_2xx: usize,
    status_4xx: usize,
    status_5xx: usize,
    empty_body: usize,
    relay_body: usize,
    other_body: usize,
    transport_errors: usize,
    skipped: usize,
}

#[derive(Debug, Clone)]
struct ResolvedTarget {
    url: Url,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let template = PacketTemplate {
        agent_id: cli.agent_id.clone(),
        session_id: cli.session_id.clone(),
        source_p_address: cli.source_p_address.clone(),
        destination: cli.destination.clone(),
        tool_name: cli.tool_name.clone(),
        seed: decode_seed(cli.seed_hex.as_str())?,
    };

    let targets = resolve_targets(&cli).await?;
    let default_payload = "{\"path\":\"/kb/index\"}".to_string();
    let default_rogue_destination = "https://203.0.113.10:443".to_string();

    match cli.mode.as_ref() {
        Some(Mode::Gauntlet {
            payload,
            rogue_destination,
        }) => {
            run_gauntlet(
                &cli,
                &template,
                &targets,
                payload.as_str(),
                rogue_destination.as_str(),
            )
            .await?
        }
        Some(Mode::Burst {
            count,
            concurrency,
            payload,
        }) => {
            run_burst(
                &cli,
                &template,
                &targets,
                *count,
                *concurrency,
                payload.as_str(),
            )
            .await?
        }
        Some(Mode::Replay { payload }) => {
            run_replay(&cli, &template, &targets, payload.as_str()).await?
        }
        Some(Mode::RogueDst {
            rogue_destination,
            payload,
        }) => {
            run_rogue_dst(
                &cli,
                &template,
                &targets,
                rogue_destination.as_str(),
                payload.as_str(),
            )
            .await?
        }
        Some(Mode::SignatureStorm { count }) => {
            run_signature_storm(&cli, &template, &targets, *count).await?
        }
        Some(Mode::SlowDrip { bytes }) => run_slow_drip(&cli, &targets, *bytes).await?,
        Some(Mode::Probe) => run_probe(&targets),
        None => {
            run_gauntlet(
                &cli,
                &template,
                &targets,
                default_payload.as_str(),
                default_rogue_destination.as_str(),
            )
            .await?
        }
    }

    Ok(())
}

async fn run_gauntlet(
    cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    payload: &str,
    rogue_destination: &str,
) -> Result<()> {
    let client = build_client(cli.timeout_ms)?;
    let mut summary = ScenarioSummary::new("gauntlet");

    let burst = run_burst_batch(
        &client,
        cli,
        template,
        targets,
        cli.burst_count,
        cli.concurrency,
        payload,
    )
    .await?;
    summary.merge(&burst);

    let replay = run_replay_batch(&client, cli, template, targets, payload).await?;
    summary.merge(&replay);

    let rogue =
        run_rogue_dst_batch(&client, cli, template, targets, rogue_destination, payload).await?;
    summary.merge(&rogue);

    let storm =
        run_signature_storm_batch(&client, cli, template, targets, cli.rounds.max(8)).await?;
    summary.merge(&storm);

    let drip = run_slow_drip_batch(targets, cli.slow_drip_bytes).await;
    match drip {
        Ok(outcome) => summary.record(outcome),
        Err(_err) => summary.record(RequestOutcome::TransportError),
    }

    print_summary(
        &summary,
        vec![
            ("targets", targets.len().to_string()),
            ("burst_count", cli.burst_count.to_string()),
            ("rounds", cli.rounds.to_string()),
        ],
    );
    Ok(())
}

async fn run_burst(
    cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    count: usize,
    concurrency: usize,
    payload: &str,
) -> Result<()> {
    let client = build_client(cli.timeout_ms)?;
    let summary =
        run_burst_batch(&client, cli, template, targets, count, concurrency, payload).await?;
    print_summary(
        &summary,
        vec![
            ("count", count.to_string()),
            ("concurrency", concurrency.to_string()),
            ("targets", targets.len().to_string()),
        ],
    );
    Ok(())
}

async fn run_replay(
    cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    payload: &str,
) -> Result<()> {
    let client = build_client(cli.timeout_ms)?;
    let summary = run_replay_batch(&client, cli, template, targets, payload).await?;
    print_summary(&summary, vec![("targets", targets.len().to_string())]);
    Ok(())
}

async fn run_rogue_dst(
    cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    rogue_destination: &str,
    payload: &str,
) -> Result<()> {
    let client = build_client(cli.timeout_ms)?;
    let summary =
        run_rogue_dst_batch(&client, cli, template, targets, rogue_destination, payload).await?;
    print_summary(
        &summary,
        vec![
            ("targets", targets.len().to_string()),
            ("rogue_destination", rogue_destination.to_string()),
        ],
    );
    Ok(())
}

async fn run_signature_storm(
    cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    count: usize,
) -> Result<()> {
    let client = build_client(cli.timeout_ms)?;
    let summary = run_signature_storm_batch(&client, cli, template, targets, count).await?;
    print_summary(
        &summary,
        vec![
            ("count", count.to_string()),
            ("targets", targets.len().to_string()),
        ],
    );
    Ok(())
}

async fn run_slow_drip(cli: &Cli, targets: &[ResolvedTarget], bytes: usize) -> Result<()> {
    let outcome = run_slow_drip_batch(targets, bytes).await?;
    let mut summary = ScenarioSummary::new("slow_drip");
    summary.record(outcome);
    print_summary(
        &summary,
        vec![
            ("bytes", bytes.to_string()),
            ("timeout_ms", cli.timeout_ms.to_string()),
        ],
    );
    Ok(())
}

fn run_probe(targets: &[ResolvedTarget]) {
    let mut summary = ScenarioSummary::new("probe");
    for _target in targets {
        summary.record(RequestOutcome::Skipped);
    }
    print_summary(&summary, vec![("targets", targets.len().to_string())]);
}

async fn run_burst_batch(
    client: &Client,
    cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    count: usize,
    concurrency: usize,
    payload: &str,
) -> Result<ScenarioSummary> {
    ensure_targets(targets)?;
    let limiter = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut tasks = JoinSet::new();

    for idx in 0..count {
        let permit = limiter.clone().acquire_owned().await?;
        let endpoint = targets[idx % targets.len()].url.clone();
        let client = client.clone();
        let packet = build_packet(
            template,
            template.destination.as_str(),
            payload,
            &format!("burst-{}-{}", unix_ms_now(), idx),
            unix_ms_now(),
        )?;
        tasks.spawn(async move {
            let _permit = permit;
            send_packet(&client, endpoint.as_str(), &packet).await
        });
    }

    let mut summary = ScenarioSummary::new("burst");
    while let Some(joined) = tasks.join_next().await {
        let outcome = joined.map_err(|e| anyhow!("burst task failed: {}", e))?;
        summary.record(outcome);
    }
    if cli.timeout_ms == 0 {
        summary.record(RequestOutcome::TransportError);
    }
    Ok(summary)
}

async fn run_replay_batch(
    client: &Client,
    _cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    payload: &str,
) -> Result<ScenarioSummary> {
    ensure_targets(targets)?;
    let target = &targets[0].url;
    let timestamp = unix_ms_now();
    let packet = build_packet(
        template,
        template.destination.as_str(),
        payload,
        "replay-fixed-nonce",
        timestamp,
    )?;

    let warm = send_packet(client, target.as_str(), &packet).await;
    let cold = send_packet(client, target.as_str(), &mutated_signature_packet(&packet)?).await;

    let mut summary = ScenarioSummary::new("replay");
    summary.record(warm);
    summary.record(cold);
    Ok(summary)
}

async fn run_rogue_dst_batch(
    client: &Client,
    _cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    rogue_destination: &str,
    payload: &str,
) -> Result<ScenarioSummary> {
    ensure_targets(targets)?;
    let target = &targets[0].url;
    let packet = build_packet(
        template,
        rogue_destination,
        payload,
        &format!("rogue-{}", unix_ms_now()),
        unix_ms_now(),
    )?;
    let mut summary = ScenarioSummary::new("rogue_dst");
    summary.record(send_packet(client, target.as_str(), &packet).await);
    Ok(summary)
}

async fn run_signature_storm_batch(
    client: &Client,
    _cli: &Cli,
    template: &PacketTemplate,
    targets: &[ResolvedTarget],
    count: usize,
) -> Result<ScenarioSummary> {
    ensure_targets(targets)?;
    let limiter = Arc::new(Semaphore::new(8));
    let mut tasks = JoinSet::new();
    let target = targets[0].url.clone();

    for idx in 0..count {
        let permit = limiter.clone().acquire_owned().await?;
        let client = client.clone();
        let packet = build_packet(
            template,
            template.destination.as_str(),
            &format!("{{\"storm\":{},\"entropy\":{}}}", idx, unix_ms_now()),
            &format!("storm-{}", idx),
            unix_ms_now(),
        )?;
        let packet = mutated_signature_packet(&packet)?;
        let endpoint = target.clone();
        tasks.spawn(async move {
            let _permit = permit;
            send_packet(&client, endpoint.as_str(), &packet).await
        });
    }

    let mut summary = ScenarioSummary::new("signature_storm");
    while let Some(joined) = tasks.join_next().await {
        let outcome = joined.map_err(|e| anyhow!("signature storm task failed: {}", e))?;
        summary.record(outcome);
    }
    Ok(summary)
}

async fn run_slow_drip_batch(targets: &[ResolvedTarget], bytes: usize) -> Result<RequestOutcome> {
    ensure_targets(targets)?;
    let target = &targets[0].url;
    if target.scheme() != "http" {
        return Ok(RequestOutcome::Skipped);
    }
    let host = target.host_str().ok_or_else(|| anyhow!("missing host"))?;
    let port = target
        .port_or_known_default()
        .ok_or_else(|| anyhow!("missing port"))?;
    let mut stream = TcpStream::connect((host, port))
        .await
        .with_context(|| format!("connect slow-drip target {}", target))?;
    let path = target.path().if_empty("/")?.to_string();
    let body = "A".repeat(bytes.max(1));
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        path,
        host,
        body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    for byte in body.bytes() {
        stream.write_all(&[byte]).await?;
        tokio::time::sleep(Duration::from_millis(350)).await;
    }
    let mut response = Vec::new();
    let _ = tokio::time::timeout(
        Duration::from_millis(2_000),
        stream.read_to_end(&mut response),
    )
    .await;
    let text = String::from_utf8_lossy(&response).to_string();
    if text.is_empty() {
        Ok(RequestOutcome::Timeout)
    } else {
        Ok(RequestOutcome::Http {
            status: 0,
            body: text,
        })
    }
}

fn ensure_targets(targets: &[ResolvedTarget]) -> Result<()> {
    if targets.is_empty() {
        Err(anyhow!(
            "no reachable endpoints; set --endpoint <url>[,<url>...] or KAIRO_STRIKER_ENDPOINTS"
        ))
    } else {
        Ok(())
    }
}

async fn resolve_targets(cli: &Cli) -> Result<Vec<ResolvedTarget>> {
    let raw = env::var("KAIRO_STRIKER_ENDPOINTS").ok();
    let spec = if cli.endpoint.trim().eq_ignore_ascii_case("auto") {
        raw.unwrap_or_else(|| AUTO_ENDPOINTS.join(","))
    } else {
        cli.endpoint.clone()
    };

    let mut targets = Vec::new();
    for candidate in split_endpoints(&spec) {
        let url =
            Url::parse(&candidate).with_context(|| format!("parse endpoint {}", candidate))?;
        if is_reachable(&url).await {
            targets.push(ResolvedTarget { url });
        }
    }

    if targets.is_empty() {
        Err(anyhow!("no reachable endpoints discovered from [{}]", spec))
    } else {
        Ok(targets)
    }
}

fn split_endpoints(spec: &str) -> Vec<String> {
    spec.split(|c| matches!(c, ',' | ';' | '\n' | '\t' | ' '))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

async fn is_reachable(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let Some(port) = url.port_or_known_default() else {
        return false;
    };
    matches!(
        tokio::time::timeout(Duration::from_millis(150), TcpStream::connect((host, port))).await,
        Ok(Ok(_))
    )
}

fn build_client(timeout_ms: u64) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .pool_max_idle_per_host(0)
        .build()
        .context("build reqwest client")
}

async fn send_packet(client: &Client, endpoint: &str, packet: &AiTcpPacket) -> RequestOutcome {
    match client.post(endpoint).json(packet).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            RequestOutcome::Http { status, body }
        }
        Err(err) if err.is_timeout() => RequestOutcome::Timeout,
        Err(_err) => RequestOutcome::TransportError,
    }
}

fn build_packet(
    template: &PacketTemplate,
    destination_p_address: &str,
    payload: &str,
    nonce: &str,
    timestamp_utc: u64,
) -> Result<AiTcpPacket> {
    let mut packet = AiTcpPacket {
        source: template.agent_id.clone(),
        destination: destination_p_address.to_string(),
        version: 1,
        source_p_address: template.source_p_address.clone(),
        destination_p_address: destination_p_address.to_string(),
        source_public_key: String::new(),
        sequence: timestamp_utc,
        agent_id: template.agent_id.clone(),
        session_id: template.session_id.clone(),
        timestamp_utc,
        nonce: nonce.to_string(),
        payload_type: template.tool_name.clone(),
        payload: payload.to_string(),
        signature: String::new(),
    };
    sign_packet(&mut packet, template.seed)?;
    Ok(packet)
}

fn mutated_signature_packet(packet: &AiTcpPacket) -> Result<AiTcpPacket> {
    let mut cloned = packet.clone();
    if cloned.signature.is_empty() {
        return Err(anyhow!("packet signature missing"));
    }
    let mut chars: Vec<char> = cloned.signature.chars().collect();
    let last = chars.len().saturating_sub(1);
    chars[last] = if chars[last] == '0' { 'f' } else { '0' };
    cloned.signature = chars.into_iter().collect();
    Ok(cloned)
}

fn sign_packet(packet: &mut AiTcpPacket, seed: [u8; 32]) -> Result<()> {
    let (pk, sk) = fips204::ml_dsa_44::KG::keygen_from_seed(&seed);
    packet.source_public_key = hex::encode(pk.into_bytes());
    let hash = packet.canonical_hash();
    let message = pq::mldsa_signing_message(&hash, PqDomain::KairoPolicy);
    let raw = sk
        .try_sign_with_seed(&seed, &message, &[])
        .map_err(|e| anyhow!("sign packet: {:?}", e))?;

    let mut signature = PqSignatureBlob {
        algorithm: PqSignatureAlgorithm::MlDsa44,
        domain: PqDomain::KairoPolicy,
        length: raw.len() as u16,
        bytes: [0u8; PQ_SIGNATURE_PLACEHOLDER_LEN],
    };
    signature.bytes[..raw.len()].copy_from_slice(&raw);
    packet.signature = encode_signature_blob_hex(&signature);
    Ok(())
}

fn encode_signature_blob_hex(signature: &PqSignatureBlob) -> String {
    let mut raw = Vec::with_capacity(4 + signature.length as usize);
    raw.push(signature.algorithm as u8);
    raw.push(signature.domain as u8);
    raw.extend_from_slice(&signature.length.to_le_bytes());
    raw.extend_from_slice(&signature.bytes[..signature.length as usize]);
    hex::encode(raw)
}

fn decode_seed(seed_hex: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(seed_hex).with_context(|| format!("invalid seed hex: {}", seed_hex))?;
    if bytes.len() != 32 {
        return Err(anyhow!("seed must be exactly 32 bytes"));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl ScenarioSummary {
    fn new(mode: &str) -> Self {
        Self {
            mode: mode.to_string(),
            ..Self::default()
        }
    }

    fn record(&mut self, outcome: RequestOutcome) {
        self.sent += 1;
        match outcome {
            RequestOutcome::Timeout => self.timeouts += 1,
            RequestOutcome::Skipped => self.skipped += 1,
            RequestOutcome::Http { status, body } => {
                if status == 200 {
                    self.status_200 += 1;
                }
                if (200..300).contains(&status) {
                    self.status_2xx += 1;
                } else if (400..500).contains(&status) {
                    self.status_4xx += 1;
                } else if status >= 500 {
                    self.status_5xx += 1;
                }

                let trimmed = body.trim();
                if trimmed.is_empty() || trimmed == "\"\"" {
                    self.empty_body += 1;
                } else if trimmed.contains("Packet relayed") || trimmed.contains("GPT processed") {
                    self.relay_body += 1;
                } else {
                    self.other_body += 1;
                }
            }
            RequestOutcome::TransportError => self.transport_errors += 1,
        }
    }

    fn merge(&mut self, other: &ScenarioSummary) {
        self.sent += other.sent;
        self.timeouts += other.timeouts;
        self.status_200 += other.status_200;
        self.status_2xx += other.status_2xx;
        self.status_4xx += other.status_4xx;
        self.status_5xx += other.status_5xx;
        self.empty_body += other.empty_body;
        self.relay_body += other.relay_body;
        self.other_body += other.other_body;
        self.transport_errors += other.transport_errors;
        self.skipped += other.skipped;
    }
}

fn print_summary(summary: &ScenarioSummary, extra: Vec<(&'static str, String)>) {
    println!("mode={}", summary.mode);
    println!("sent={}", summary.sent);
    println!("timeouts={}", summary.timeouts);
    println!("status_200={}", summary.status_200);
    println!("status_2xx={}", summary.status_2xx);
    println!("status_4xx={}", summary.status_4xx);
    println!("status_5xx={}", summary.status_5xx);
    println!("empty_body={}", summary.empty_body);
    println!("relay_body={}", summary.relay_body);
    println!("other_body={}", summary.other_body);
    println!("transport_errors={}", summary.transport_errors);
    println!("skipped={}", summary.skipped);
    for (key, value) in extra {
        println!("{}={}", key, value);
    }
}

trait PathExt {
    fn if_empty<'a>(&'a self, default: &'a str) -> Result<&'a str>;
}

impl PathExt for str {
    fn if_empty<'a>(&'a self, default: &'a str) -> Result<&'a str> {
        if self.is_empty() {
            Ok(default)
        } else {
            Ok(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_endpoints_breaks_comma_lists() {
        let endpoints = split_endpoints("http://a:1/send, http://b:2/send;http://c:3/send");
        assert_eq!(endpoints.len(), 3);
    }

    #[test]
    fn mutate_signature_changes_tail() {
        let packet = AiTcpPacket {
            source: "a".into(),
            destination: "b".into(),
            version: 1,
            source_p_address: "1.1.1.1:1".into(),
            destination_p_address: "2.2.2.2:2".into(),
            source_public_key: String::new(),
            sequence: 1,
            agent_id: "a".into(),
            session_id: "s".into(),
            timestamp_utc: 1,
            nonce: "n".into(),
            payload_type: "tool".into(),
            payload: "{}".into(),
            signature: "abcd".into(),
        };
        let mutated = mutated_signature_packet(&packet).unwrap();
        assert_ne!(mutated.signature, packet.signature);
    }
}
