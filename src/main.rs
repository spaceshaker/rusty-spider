use crate::crawler_progress_reporter::CrawlerProgressReporter;
use anyhow::anyhow;
use clap::Parser;
use console_progress_reporter::ConsoleProcessReporter;
use crawler_state::CrawlerState;
use progress_reporter::ProgressReporter;
use robots_txt::Robots;
use std::collections::HashSet;
use std::process;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::task::JoinHandle;
use url::Url;

mod console_progress_reporter;
mod crawler_progress_event;
mod crawler_progress_reporter;
mod crawler_state;
mod progress_reporter;

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
struct PageSummary {
    url: Url,
    status_code: u16,
    content_type: String,
    title: String,
    num_outgoing_links: usize,
}

impl PageSummary {
    fn new(
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
}

#[derive(Debug, Clone)]
struct CrawlSummary {
    crawl_summaries: Vec<PageSummary>,
}

impl CrawlSummary {
    fn new(crawl_summaries: Vec<PageSummary>) -> Self {
        Self { crawl_summaries }
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

async fn crawl_single_url(crawl_request: CrawlRequest) -> Result<CrawlResponse, CrawlError> {
    let url_to_crawl = &crawl_request.url;

    let crawl_response = reqwest::get(url_to_crawl.clone()).await?;
    if !crawl_response.status().is_success() {
        println!(
            "Failed to fetch {}: {}",
            url_to_crawl,
            crawl_response.status()
        );
        return Err(CrawlError::HttpError(crawl_response.status().as_u16()));
    }
    let status_code = crawl_response.status().as_u16();

    let content_type_str = crawl_response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let content_type: mime::Mime = content_type_str.clone().parse()?;
    match (content_type.type_(), content_type.subtype()) {
        (mime::TEXT, mime::HTML) => {}
        _ => {
            println!("Skipping non-HTML content type: {}", content_type);
            return Err(CrawlError::AnyError(anyhow!(
                "Skipping non-HTML content type: {}",
                content_type
            )));
        }
    }

    let html_text = crawl_response.text().await?;
    let document = scraper::Html::parse_document(html_text.as_str());

    let title = {
        let title_selector = scraper::Selector::parse("title").unwrap();
        if let Some(title_element) = document.select(&title_selector).next() {
            let title = title_element.inner_html();
            Some(title)
        } else {
            None
        }
    };

    let mut discovered_urls: HashSet<Url> = HashSet::new();
    let link_selector = scraper::Selector::parse("a[href]").unwrap();
    for element in document.select(&link_selector) {
        if let Some(link) = element.value().attr("href") {
            let url = {
                if link.starts_with("/") {
                    let mut new_url = url_to_crawl.clone();
                    new_url.set_path(link);
                    new_url
                } else if link.starts_with("#") {
                    continue; // Ignore fragment links
                } else if link.starts_with("mailto:") {
                    continue; // Ignore mailto links
                } else if link.starts_with("javascript:") {
                    continue; // Ignore javascript links
                } else if link.starts_with("tel:") {
                    continue; // Ignore tel links
                } else {
                    if let Ok(link_url) = Url::parse(link) {
                        link_url
                    } else {
                        continue;
                    }
                }
            };
            discovered_urls.insert(url);
        }
    }

    let mut external_urls: Vec<Url> = Vec::new();
    let mut internal_urls: Vec<Url> = Vec::new();
    for discovered_url in discovered_urls {
        if discovered_url.has_host()
            && discovered_url.host().unwrap() == url_to_crawl.host().unwrap()
        {
            internal_urls.push(discovered_url);
        } else {
            external_urls.push(discovered_url);
        }
    }

    let result = CrawlResponse {
        url: url_to_crawl.clone(),
        status_code,
        content_type: content_type_str,
        title: title.unwrap_or_else(|| "No title".to_string()),
        outgoing_links: external_urls,
        internal_links: internal_urls,
    };
    Ok(result)
}

#[derive(Debug, Clone)]
struct CrawlerConfig {
    max_pages: usize,
    max_depth: usize,
    requests_per_second: Option<f64>,
}

async fn crawl_seed(
    seed: &Url,
    config: &CrawlerConfig,
    progress_reporter: impl ProgressReporter,
) -> anyhow::Result<CrawlSummary> {
    progress_reporter.begin();

    let crawl_delay: Option<tokio::time::Duration> = {
        if let Some(requests_per_second) = config.requests_per_second {
            let crawl_delay_in_ms = (1000.0 / requests_per_second) as u64;
            Some(tokio::time::Duration::from_millis(crawl_delay_in_ms))    
        } else {
            None
        }
    };

    let mut crawl_summaries: Vec<PageSummary> = Vec::new();

    let robots_text = {
        let robots_url = seed.join("/robots.txt")?;
        let robots_response = reqwest::get(robots_url.clone()).await?;
        if robots_response.status().is_success() {
            let robots_text = robots_response.text().await?;
            Ok::<String, anyhow::Error>(robots_text)
        } else {
            return Err(anyhow!("Failed to fetch robots.txt"));
        }
    }?;
    let robots = Robots::from_str_lossy(robots_text.as_str());
    let robots_matcher =
        robots_txt::matcher::SimpleMatcher::new(&robots.choose_section("rusty-spider").rules);

    let mut urls_already_crawled = HashSet::new();
    let mut urls_to_crawl = vec![seed.clone()];
    while !urls_to_crawl.is_empty() {
        if crawl_summaries.len() > config.max_pages {
            break;
        }

        progress_reporter.progress_update(urls_to_crawl.len(), urls_already_crawled.len());

        let url_to_crawl = urls_to_crawl.pop().unwrap();

        if urls_already_crawled.contains(&url_to_crawl) {
            continue;
        }
        urls_already_crawled.insert(url_to_crawl.clone());

        if !robots_matcher.check_path(url_to_crawl.path()) {
            println!("Crawling is disallowed by robots.txt for {}", url_to_crawl);
            continue;
        }

        progress_reporter.crawler_state_changed(CrawlerState::Crawling);

        let crawl_request = CrawlRequest {
            url: url_to_crawl.clone(),
        };
        let crawl_response = crawl_single_url(crawl_request).await;
        let page_summary = match crawl_response {
            Ok(crawl_response) => {
                for internal_link in crawl_response.internal_links {
                    if urls_already_crawled.contains(&internal_link) {
                        continue;
                    }
                    urls_to_crawl.push(internal_link);
                }

                PageSummary::new(
                    crawl_response.url,
                    crawl_response.status_code,
                    crawl_response.content_type,
                    crawl_response.title,
                    crawl_response.outgoing_links.len(),
                )
            }
            Err(e) => {
                let status_code = match e {
                    CrawlError::HttpError(status_code) => status_code,
                    _ => 500u16,
                };
                PageSummary::new(
                    url_to_crawl.clone(),
                    status_code,
                    String::new(),
                    String::new(),
                    0,
                )
            }
        };
        crawl_summaries.push(page_summary);

        if let Some(crawl_delay) = crawl_delay {
            if !urls_to_crawl.is_empty() {
                progress_reporter.crawler_state_changed(CrawlerState::Paused);
                tokio::time::sleep(crawl_delay).await;
            }    
        }
    }

    progress_reporter.end();

    Ok(CrawlSummary::new(crawl_summaries))
}

async fn main_impl(args: &CommandLineArgs) -> anyhow::Result<()> {
    let crawler_config = CrawlerConfig {
        max_pages: args.max_pages,
        max_depth: args.max_depth,
        requests_per_second: args.rate,
    };

    let mut r: Vec<CrawlSummary> = Vec::new();
    {
        let handles: Vec<JoinHandle<anyhow::Result<CrawlSummary>>> = Vec::new();
        let console_reporter = ConsoleProcessReporter::new();
        let mut results: Vec<JoinHandle<anyhow::Result<CrawlSummary>>> = Vec::new();
        let crawler_index = Arc::new(AtomicUsize::new(0));
        for seed_str in &args.seed {
            let seed_url = Url::parse(seed_str)?;

            let crawler_config = crawler_config.clone();
            let crawler_index = Arc::clone(&crawler_index);
            let console_reporter = console_reporter.clone();
            let handle = tokio::task::spawn(async move {
                let crawler_index = crawler_index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let progress_reporter = CrawlerProgressReporter::new(
                    crawler_index,
                    seed_url.clone(),
                    console_reporter.event_tx(),
                );
                let crawl_summary = crawl_seed(&seed_url, &crawler_config, progress_reporter).await?;
                Ok::<CrawlSummary, anyhow::Error>(crawl_summary)
            });
            results.push(handle);
        }

        for handle in results {
            let result = handle.await??;
            r.push(result);
        }
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
