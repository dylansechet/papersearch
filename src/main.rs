mod core;
use clap::Parser;
use core::{fetch_citation_graph, Config, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Build citation graphs from academic paper references
#[derive(Parser, Debug)]
#[command(name = "papersearch", version, about)]
struct Cli {
    /// Input file containing paper references (DOIs, arXiv IDs, URLs)
    r#in: PathBuf,

    /// Output JSON file for the citation graph
    #[arg(short, long)]
    out: PathBuf,

    /// Semantic Scholar API key (optional, increases rate limits)
    #[arg(short = 'k', long)]
    api_key: Option<String>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();

    let config = Config {
        api_key: cli.api_key.clone(),
        rate_limit_secs: if cli.api_key.is_some() { 2 } else { 5 },
    };

    info!("Reading paper references from {:?}...", cli.r#in);
    let references: Vec<String> = BufReader::new(fs::File::open(&cli.r#in)?)
        .lines()
        .map_while(|r| r.ok())
        .filter(|line| !line.trim().is_empty())
        .collect();

    info!("Building citation graph...");
    let graph = fetch_citation_graph(references, config)?;

    let stats = graph.stats();

    info!("Writing to {:?}...", cli.out);
    fs::write(&cli.out, serde_json::to_string_pretty(&graph)?)?;

    info!(
        "Complete! {} total papers ({} seed) | {} citing works | {} references",
        stats.total_papers, stats.seed_papers, stats.citing_works, stats.references
    );

    Ok(())
}
