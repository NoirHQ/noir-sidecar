#[tokio::main]
async fn main() -> anyhow::Result<()> {
    noir_sidecar::logger::enable_logger();

    tracing::info!("Starting sidecar");

    Ok(())
}
