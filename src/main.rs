mod core;
use clap::{Args, Parser, Subcommand};
use core::{fetch_citation_graph, Config, Result};
use rust_embed::Embed;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tiny_http::{Header, Response};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Embed)]
#[folder = "webui/dist"]
struct Assets;

/// Build citation graphs from academic paper references
#[derive(Parser, Debug)]
#[command(name = "papersearch", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Look up and build a citation graph from paper references
    Lookup(LookupArgs),
    /// Launch the Svelte web UI
    View(ViewArgs),
}

#[derive(Args, Debug)]
struct LookupArgs {
    /// Input file containing paper references (DOIs, arXiv IDs, URLs)
    r#in: PathBuf,

    /// Output JSON file for the citation graph (prints to stdout if not specified)
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Semantic Scholar API key (optional, increases rate limits)
    #[arg(short = 'k', long)]
    api_key: Option<String>,
}

#[derive(Args, Debug)]
struct ViewArgs {
    /// Path to a JSON citation graph file to load (optional)
    graph: Option<PathBuf>,

    /// Port to serve the web UI on
    #[arg(short, long, default_value_t = 4173)]
    port: u16,

    /// Do not open a browser tab after the server starts
    #[arg(long)]
    no_open: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Lookup(args) => run_lookup(args),
        Commands::View(args) => run_view(args),
    }
}

fn run_lookup(args: LookupArgs) -> Result<()> {
    let config = Config {
        api_key: args.api_key.clone(),
        rate_limit_secs: if args.api_key.is_some() { 2 } else { 5 },
    };

    info!("Reading paper references from {:?}...", args.r#in);
    let references: Vec<String> = BufReader::new(fs::File::open(&args.r#in)?)
        .lines()
        .map_while(|r| r.ok())
        .filter(|line| !line.trim().is_empty())
        .collect();

    info!("Building citation graph...");
    let graph = fetch_citation_graph(references, config)?;

    let stats = graph.stats();
    let json = serde_json::to_string_pretty(&graph)?;

    if let Some(out_path) = args.out {
        info!("Writing to {:?}...", out_path);
        fs::write(&out_path, &json)?;
    } else {
        println!("{}", json);
    }

    info!(
        "Complete! {} total papers ({} seed) | {} citing works | {} references",
        stats.total_papers, stats.seed_papers, stats.citing_works, stats.references
    );

    Ok(())
}

fn run_view(args: ViewArgs) -> Result<()> {
    let addr = format!("127.0.0.1:{}", args.port);
    let graph_path = if let Some(gp) = &args.graph {
        Some(gp.canonicalize()?)
    } else {
        None
    };

    let url = if graph_path.is_some() {
        format!("http://{addr}/?graph=true")
    } else {
        format!("http://{addr}/")
    };
    info!("Starting web UI on {} (Ctrl+C to stop)", url);

    if !args.no_open {
        if let Err(err) = webbrowser::open(&url) {
            warn!("Could not open browser automatically: {}", err);
        }
    }

    let server =
        tiny_http::Server::http(&addr).map_err(|e| core::Error::Io(std::io::Error::other(e)))?;
    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/");

        // Handle API endpoint for graph file
        if path == "/api/graph" {
            if let Some(ref gpath) = graph_path {
                match fs::read(gpath) {
                    Ok(data) => {
                        let mut response = Response::from_data(data).with_status_code(200);
                        if let Ok(header) = Header::from_bytes(b"Content-Type", b"application/json")
                        {
                            response = response.with_header(header);
                        }
                        if let Err(e) = request.respond(response) {
                            warn!("Failed to respond: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read graph file: {}", e);
                        let response =
                            Response::from_string("Graph file not found").with_status_code(404);
                        let _ = request.respond(response);
                    }
                }
            } else {
                let response = Response::from_string("No graph file loaded").with_status_code(400);
                let _ = request.respond(response);
            }
            continue;
        }

        let file_path = path.trim_start_matches('/');
        let file_path = if file_path.is_empty() {
            "index.html"
        } else {
            file_path
        };

        let (data, content_type) = match Assets::get(file_path) {
            Some(asset) => {
                let mime = mime_guess::from_path(file_path)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string();
                (asset.data.into_owned(), mime)
            }
            None => {
                // Fallback to index.html for SPA routing
                match Assets::get("index.html") {
                    Some(asset) => {
                        let mime = "text/html; charset=utf-8".to_string();
                        (asset.data.into_owned(), mime)
                    }
                    None => {
                        let response = Response::from_string("Not found").with_status_code(404);
                        if let Err(e) = request.respond(response) {
                            warn!("Failed to respond: {}", e);
                        }
                        continue;
                    }
                }
            }
        };

        let mut response = Response::from_data(data).with_status_code(200);
        if let Ok(header) = Header::from_bytes(b"Content-Type", content_type.as_bytes()) {
            response = response.with_header(header);
        }
        if let Err(e) = request.respond(response) {
            warn!("Failed to respond: {}", e);
        }
    }

    Ok(())
}
