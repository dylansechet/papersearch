use backoff::{backoff::Backoff, ExponentialBackoff};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::blocking::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use thiserror::Error;
use tracing::{debug, info, warn};
use url::Url;

#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unrecognized reference: {0}")]
    UnrecognizedFormat(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// API response types
#[derive(Deserialize)]
struct Author {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPaper {
    paper_id: Option<String>,
    title: Option<String>,
    #[serde(default)]
    authors: Vec<Author>,
    venue: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    citations: Vec<ApiPaper>,
    #[serde(default)]
    references: Vec<ApiPaper>,
    citation_count: Option<u32>,
    reference_count: Option<u32>,
}

// Output types
#[derive(Serialize)]
pub struct Paper {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    authors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    venue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
}

#[derive(Serialize)]
pub struct Citation {
    from: String,
    to: String,
}

#[derive(Serialize)]
pub struct Graph {
    pub papers: Vec<Paper>,
    pub seeds: Vec<String>,
    pub citations: Vec<Citation>,
}

#[derive(Debug)]
pub struct GraphStats {
    pub total_papers: usize,
    pub seed_papers: usize,
    pub citing_works: usize,
    pub references: usize,
}

impl Graph {
    pub fn stats(&self) -> GraphStats {
        let seed_set: HashSet<_> = self.seeds.iter().collect();

        // Count unique papers that cite seeds
        let citing_works: HashSet<_> = self
            .citations
            .iter()
            .filter(|c| seed_set.contains(&c.to))
            .map(|c| &c.from)
            .collect();

        // Count unique papers cited by seeds
        let references: HashSet<_> = self
            .citations
            .iter()
            .filter(|c| seed_set.contains(&c.from))
            .map(|c| &c.to)
            .collect();

        GraphStats {
            total_papers: self.papers.len(),
            seed_papers: self.seeds.len(),
            citing_works: citing_works.len(),
            references: references.len(),
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub api_key: Option<String>,
    pub rate_limit_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            rate_limit_secs: 5,
        }
    }
}

static SHA_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[0-9a-f]{40}$").unwrap());
static DOI_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"10\.\d{4,9}/\S+").unwrap());
static ARXIV_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{4}\.\d{4,5}").unwrap());

const API_URL: &str = "https://api.semanticscholar.org/graph/v1/paper/batch";
const BATCH_SIZE: usize = 200;
const MAX_CONNECTIONS: u32 = 9999;

struct Client {
    http: HttpClient,
    rate_limit_secs: u64,
}

impl Client {
    fn new(config: &Config) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key) = &config.api_key {
            headers.insert("x-api-key", key.parse().unwrap());
        }

        Ok(Self {
            http: HttpClient::builder()
                .default_headers(headers)
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            rate_limit_secs: config.rate_limit_secs,
        })
    }

    fn post<T: serde::de::DeserializeOwned>(&self, fields: &str, ids: &[String]) -> Result<Vec<T>> {
        let mut backoff = ExponentialBackoff::default();
        let url = format!("{}?fields={}", API_URL, fields);

        loop {
            match self
                .http
                .post(&url)
                .json(&serde_json::json!({ "ids": ids }))
                .send()
            {
                Ok(response) if response.status().is_success() => {
                    std::thread::sleep(std::time::Duration::from_secs(self.rate_limit_secs));
                    return Ok(response.json()?);
                }
                Ok(response) => {
                    let retryable = matches!(response.status().as_u16(), 429 | 500..=504);
                    if retryable && backoff.next_backoff().is_some() {
                        std::thread::sleep(backoff.next_backoff().unwrap());
                    } else {
                        return Err(response.error_for_status().unwrap_err().into());
                    }
                }
                Err(e) if e.is_timeout() || e.is_connect() => {
                    if let Some(duration) = backoff.next_backoff() {
                        std::thread::sleep(duration);
                    } else {
                        return Err(e.into());
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

fn normalize_reference(reference: &str) -> Result<String> {
    let reference = reference.trim();

    // Already normalized or a SHA hash
    if (reference.contains(':') && !reference.starts_with("http"))
        || SHA_PATTERN.is_match(reference)
    {
        return Ok(reference.to_string());
    }

    // Handle URLs from known domains
    if let Ok(url) = Url::parse(reference) {
        if let Some(domain) = url.host_str() {
            if [
                "semanticscholar.org",
                "arxiv.org",
                "aclweb.org",
                "acm.org",
                "biorxiv.org",
            ]
            .iter()
            .any(|&d| domain.to_lowercase().contains(d))
            {
                return Ok(format!("URL:{}", reference));
            }
        }
    }

    // Try to extract DOI (from URL or plain text)
    if let Some(m) = DOI_PATTERN.find(reference) {
        return Ok(format!("DOI:{}", m.as_str()));
    }

    // Check for plain arXiv ID (only if not a URL)
    if !reference.starts_with("http") && ARXIV_PATTERN.is_match(reference) {
        if let Some(caps) = ARXIV_PATTERN.captures(reference) {
            return Ok(format!("ARXIV:{}", &caps[0]));
        }
    }

    Err(Error::UnrecognizedFormat(reference.to_string()))
}

fn batch_papers(counts: &HashMap<String, (u32, u32)>) -> Vec<Vec<String>> {
    let mut papers: Vec<_> = counts
        .iter()
        .map(|(id, &(cites, refs))| (id.clone(), cites + refs))
        .collect();
    papers.sort_unstable_by_key(|(_, cost)| std::cmp::Reverse(*cost));

    let (mut batches, mut batch, mut batch_cost) = (Vec::new(), Vec::new(), 0u32);

    for (id, cost) in papers {
        if (batch_cost + cost > MAX_CONNECTIONS || batch.len() >= BATCH_SIZE) && !batch.is_empty() {
            batches.push(std::mem::take(&mut batch));
            batch_cost = 0;
        }
        batch.push(id);
        batch_cost += cost;
    }

    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

fn fetch_counts(client: &Client, ids: &[String]) -> Result<HashMap<String, (u32, u32)>> {
    let mut counts = HashMap::new();
    for chunk in ids.chunks(BATCH_SIZE) {
        let papers: Vec<ApiPaper> = client.post("paperId,citationCount,referenceCount", chunk)?;

        for paper in papers {
            if let Some(id) = paper.paper_id {
                counts.insert(
                    id,
                    (
                        paper.citation_count.unwrap_or(0),
                        paper.reference_count.unwrap_or(0),
                    ),
                );
            }
        }
    }

    Ok(counts)
}

fn fetch_papers(client: &Client, batches: &[Vec<String>]) -> Result<HashMap<String, ApiPaper>> {
    const FIELDS: &str = "paperId,title,authors,venue,year,\
                          citations,references,\
                          citations.paperId,citations.title,citations.authors,citations.venue,citations.year,\
                          references.paperId,references.title,references.authors,references.venue,references.year";

    let mut results = HashMap::new();

    for (i, batch) in batches.iter().enumerate() {
        info!(
            "Fetching batch {}/{} ({} papers)",
            i + 1,
            batches.len(),
            batch.len()
        );

        let papers: Vec<ApiPaper> = client.post(FIELDS, batch)?;
        for paper in papers {
            if let Some(id) = paper.paper_id.clone() {
                results.insert(id, paper);
            }
        }
    }

    Ok(results)
}

fn build_graph(papers: HashMap<String, ApiPaper>, seeds: HashSet<String>) -> Graph {
    let mut citations = Vec::new();
    let mut all_papers: HashMap<String, Paper> = HashMap::new();

    // Process seed papers and their citations/references
    for (id, paper) in papers {
        // Add the seed paper itself
        all_papers.insert(
            id.clone(),
            Paper {
                id: id.clone(),
                title: paper.title,
                authors: paper.authors.iter().map(|a| a.name.clone()).collect(),
                venue: paper.venue,
                year: paper.year,
            },
        );

        // Process references and add them to papers
        for ref_paper in &paper.references {
            if let Some(ref_id) = &ref_paper.paper_id {
                citations.push(Citation {
                    from: id.clone(),
                    to: ref_id.clone(),
                });

                // Add referenced paper if not already present
                all_papers.entry(ref_id.clone()).or_insert_with(|| Paper {
                    id: ref_id.clone(),
                    title: ref_paper.title.clone(),
                    authors: ref_paper.authors.iter().map(|a| a.name.clone()).collect(),
                    venue: ref_paper.venue.clone(),
                    year: ref_paper.year,
                });
            }
        }

        // Process citations and add them to papers
        for cit_paper in &paper.citations {
            if let Some(cit_id) = &cit_paper.paper_id {
                citations.push(Citation {
                    from: cit_id.clone(),
                    to: id.clone(),
                });

                // Add citing paper if not already present
                all_papers.entry(cit_id.clone()).or_insert_with(|| Paper {
                    id: cit_id.clone(),
                    title: cit_paper.title.clone(),
                    authors: cit_paper.authors.iter().map(|a| a.name.clone()).collect(),
                    venue: cit_paper.venue.clone(),
                    year: cit_paper.year,
                });
            }
        }
    }

    Graph {
        papers: all_papers.into_values().collect(),
        seeds: seeds.into_iter().collect(),
        citations,
    }
}

pub fn fetch_citation_graph(references: Vec<String>, config: Config) -> Result<Graph> {
    let client = Client::new(&config)?;

    let ids: Vec<String> = references
        .into_iter()
        .filter_map(|line| match normalize_reference(&line) {
            Ok(id) => Some(id),
            Err(e) => {
                warn!("Skipping '{}': {}", line, e);
                None
            }
        })
        .collect();

    debug!("Normalized references {:?}", ids);
    info!("Fetching coarse reference numbers");
    let counts = fetch_counts(&client, &ids)?;
    let batches = batch_papers(&counts);
    info!("Fetching paper details");

    let papers = fetch_papers(&client, &batches)?;
    let seeds = counts.keys().cloned().collect();

    Ok(build_graph(papers, seeds))
}
