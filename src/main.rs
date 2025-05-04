use crate::crawler::SeedCrawler;
use crate::crawler_progress_reporter::CrawlerProgressReporter;
use clap::Parser;
use console_progress_reporter::ConsoleProcessReporter;
use std::process;
use std::sync::Arc;
use futures::future::join_all;
use tokio::select;
use tokio::task::JoinHandle;
use url::Url;

mod console_progress_reporter;
mod crawler_progress_event;
mod crawler_progress_reporter;
mod crawler_state;
mod progress_reporter;
mod crawler;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CommandLineArgs {
    /// Seed URLs to start crawling from
    #[arg(long, value_name = "URL")]
    seed: Vec<String>,

    /// Maximum number of pages to crawl
    #[arg(long, default_value_t = 1000)]
    max_pages: usize,

    /// Maximum depth to crawl
    #[arg(long, default_value_t = 4)]
    max_depth: usize,

    /// Rate limit for crawling (requests per second)
    #[arg(long)]
    rate: Option<f64>,
}

#[derive(Debug, Clone)]
struct CrawlRequest {
    url: Url,
}

#[derive(Debug, Clone)]
struct CrawlResponse {
    url: Url,
    status_code: u16,
    content_type: String,
    title: String,
    outgoing_links: Vec<Url>,
    internal_links: Vec<Url>,
}

#[derive(Debug, Clone)]
pub struct PageSummary {
    url: Url,
    status_code: u16,
    content_type: String,
    title: String,
    num_outgoing_links: usize,
}

impl PageSummary {
    pub fn new(
        url: Url,
        status_code: u16,
        content_type: String,
        title: String,
        num_outgoing_links: usize,
    ) -> Self {
        Self {
            url,
            status_code,
            content_type,
            title,
            num_outgoing_links,
        }
    }
    
    pub fn from_status_code(
        url: Url,
        status_code: u16,
    ) -> Self {
        Self {
            url,
            status_code,
            content_type: String::new(),
            title: String::new(),
            num_outgoing_links: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrawlSummary {
    crawl_summaries: Vec<PageSummary>,
}

impl CrawlSummary {
    pub fn new(crawl_summaries: Vec<PageSummary>) -> Self {
        Self { crawl_summaries }
    }
    
    pub fn add_page_summary(&mut self, page_summary: PageSummary) {
        self.crawl_summaries.push(page_summary);
    }
}

impl Default for CrawlSummary {
    fn default() -> Self {
        Self {
            crawl_summaries: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CrawlError {
    #[error("HTTP Error Status Code = {0}")]
    HttpError(u16),

    #[error(transparent)]
    AnyError(#[from] anyhow::Error),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error(transparent)]
    MimeParseError(#[from] mime::FromStrError),
}

#[derive(Debug, Clone)]
struct CrawlerConfig {
    max_pages: usize,
    max_depth: usize,
    requests_per_second: Option<f64>,
}

async fn main_impl(args: &CommandLineArgs) -> anyhow::Result<()> {
    let crawler_config = CrawlerConfig {
        max_pages: args.max_pages,
        max_depth: args.max_depth,
        requests_per_second: args.rate,
    };
    
    // Set up a shutdown signal handler
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    {
        let shutdown_notify = Arc::clone(&shutdown_notify);
        ctrlc::set_handler(move || {
            println!("Received Ctrl+C, shutting down...");
            shutdown_notify.notify_waiters();
        })?;
    }

    let mut r: Vec<CrawlSummary> = Vec::new();
    {
        let console_reporter = ConsoleProcessReporter::new();
        let console_reporter_task = {
            let shutdown_notify = Arc::clone(&shutdown_notify);
            let mut console_reporter = console_reporter.clone();
            tokio::task::spawn(async move {
                console_reporter.run(shutdown_notify).await.unwrap();
            })
        };
        
        let mut results: Vec<JoinHandle<anyhow::Result<CrawlSummary>>> = Vec::new();
        for (crawler_index, seed_str) in args.seed.iter().enumerate() {
            let seed_url = Url::parse(seed_str)?;

            let crawler_config = crawler_config.clone();
            let console_reporter = console_reporter.clone();
            let handle = tokio::task::spawn(async move {
                let progress_reporter = CrawlerProgressReporter::new(
                    crawler_index,
                    seed_url.clone(),
                    console_reporter.event_tx(),
                );
                let seed_crawler = SeedCrawler::new(crawler_index, seed_url, progress_reporter);
                let crawl_summary = seed_crawler.crawl(crawler_config).await?;
                Ok::<CrawlSummary, anyhow::Error>(crawl_summary)
            });
            results.push(handle);
        }
        
        let all_tasks = join_all(results);
        
        select! {
            results = all_tasks => {
                for result in results {
                    match result {
                        Ok(crawl_summary) => {
                            r.push(crawl_summary?);
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
            },
            _ = shutdown_notify.notified() => {
                // Do nothing
            }
        }
        
        
        shutdown_notify.notified().await;
    }
    
    for crawl_summary in r {
        for page_summary in crawl_summary.crawl_summaries {
            println!(
                "{}, {}, {}, {}, {}",
                page_summary.url,
                page_summary.status_code,
                page_summary.content_type,
                page_summary.title,
                page_summary.num_outgoing_links
            );
        }
    }
    
    Ok(())
}

#[tokio::main]
async fn main() {
    let args = CommandLineArgs::parse();

    if let Err(e) = main_impl(&args).await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
