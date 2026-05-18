mod nw_attack;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    nw_attack::run().await
}
