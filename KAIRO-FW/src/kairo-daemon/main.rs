#[tokio::main]
async fn main() {
    kairo_fw::run_embedded_daemon().await.unwrap();
}
