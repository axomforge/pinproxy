use clap::Parser;
use log::info;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::http::ResponseHeader;

/// A lightweight HTTP proxy server based on Pingora
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Number of worker threads
    #[arg(short, long, default_value = "2")]
    workers: usize,

    /// Enable daemon mode
    #[arg(short, long)]
    daemon: bool,
}

pub struct ProxyService;

#[async_trait::async_trait]
impl ProxyHttp for ProxyService {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // Extract the host from the request headers
        let host = session
            .req_header()
            .headers
            .get("Host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("example.com:80");

        info!("Proxying request to: {}", host);

        // Parse host and port
        let (hostname, port) = if host.contains(':') {
            let parts: Vec<&str> = host.split(':').collect();
            (parts[0], parts[1].parse().unwrap_or(80))
        } else {
            (host, 80)
        };

        let peer = Box::new(HttpPeer::new(
            (hostname, port),
            false, // TLS
            hostname.to_string(),
        ));

        Ok(peer)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Remove proxy-specific headers if present
        upstream_request.remove_header("Proxy-Connection");
        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Add custom header to identify the proxy
        upstream_response
            .insert_header("X-Proxy-Server", "pinproxy")
            .unwrap();
        Ok(())
    }

    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&pingora::Error>,
        _ctx: &mut Self::CTX,
    ) {
        let req = session.req_header();
        info!(
            "{} {} {} - Status: {}",
            session.client_addr().unwrap_or(&"unknown".parse().unwrap()),
            req.method,
            req.uri,
            session
                .response_written()
                .map(|r| r.status.as_u16())
                .unwrap_or(0)
        );
    }
}

fn main() {
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    // Parse command line arguments
    let args = Args::parse();

    info!("Starting pinproxy on port {}", args.port);
    info!("Workers: {}", args.workers);

    // Create Pingora server
    let mut server = Server::new(Some(Opt {
        upgrade: false,
        daemon: args.daemon,
        nocapture: false,
        test: false,
        conf: None,
    }))
    .unwrap();

    server.bootstrap();

    // Create proxy service - ProxyService itself, not Arc
    let proxy_service = ProxyService;

    let mut proxy_service_builder = http_proxy_service(&server.configuration, proxy_service);
    proxy_service_builder.add_tcp(&format!("0.0.0.0:{}", args.port));

    server.add_service(proxy_service_builder);

    info!("Proxy server ready to accept connections");

    // Run the server
    server.run_forever();
}
