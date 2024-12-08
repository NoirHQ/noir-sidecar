#[tokio::main]
async fn main() -> anyhow::Result<()> {
    noir_sidecar::logger::enable_logger();

    let args = noir_sidecar::cli::parse_args();
    let config = noir_sidecar::config::read_config(&args.config)?;

    tracing::trace!("config: {:#?}", config);

    let server = noir_sidecar::server::SidecarServer::new(config.server);
    server.run(noir_sidecar::router::create_router()).await?;

    Ok(())
}
