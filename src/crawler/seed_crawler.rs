use crate::crawler::{RobotsTxtMatcher, RobotsTxtSource};
use crate::crawler_state::CrawlerState;
use crate::progress_reporter::ProgressReporter;
use crate::{CrawlError, CrawlRequest, CrawlResponse, CrawlSummary, CrawlerConfig, PageSummary};
use anyhow::anyhow;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use url::Url;

#[derive(Clone)]
struct CrawlContext {
    urls_to_crawl: HashSet<Url>,
    urls_already_crawled: HashSet<Url>,
}

impl CrawlContext {
    pub fn new() -> Self {
        Self {
            urls_to_crawl: HashSet::new(),
            urls_already_crawled: HashSet::new(),
        }
    }

    pub fn add_url_to_crawl(&mut self, url: &Url) {
        let stripped_url = self.strip_url(url);
        if !self.urls_already_crawled.contains(&stripped_url) {
            self.urls_to_crawl.insert(stripped_url);
        }
    }

    pub fn add_urls_to_crawl(&mut self, urls: &[Url]) {
        for url in urls {
            self.add_url_to_crawl(url);
        }
    }

    pub fn pop_url_to_crawl(&mut self) -> Option<Url> {
        self.urls_to_crawl.iter().next().cloned().and_then(|url| {self.urls_to_crawl.take(&url)})
    }

    pub fn mark_url_as_crawled(&mut self, url: &Url) {
        let stripped_url = self.strip_url(url);
        self.urls_to_crawl.remove(&stripped_url);
        self.urls_already_crawled.insert(stripped_url);
    }

    pub fn is_crawling_complete(&self) -> bool {
        self.urls_to_crawl.is_empty()
    }

    pub fn progress(&self) -> (usize, usize) {
        let num_urls_to_crawl = self.urls_to_crawl.len();
        let num_urls_crawled = self.urls_already_crawled.len();
        (num_urls_to_crawl, num_urls_crawled)
    }

    /// Strips the URL of its fragment and query components.
    fn strip_url(&self, url: &Url) -> Url {
        let mut stripped_url = url.clone();
        stripped_url.set_fragment(None);
        stripped_url.set_query(None);
        stripped_url
    }
}

impl Default for CrawlContext {
    fn default() -> Self {
        Self {
            urls_to_crawl: HashSet::new(),
            urls_already_crawled: HashSet::new(),
        }
    }
}

#[derive(Clone)]
pub enum PageCrawlOutput {
    NoMoreUrlsToCrawl,
    DeniedByRobotsTxt(Url),
    HttpNotFound(Url),
    HttpError(Url, u16),
    Success(PageSummary),
}

pub struct SeedCrawler<TP>
where
    TP: ProgressReporter,
{
    shutdown_notify: Arc<tokio::sync::Notify>,
    index: usize,
    seed: Url,
    progress_reporter: TP,
}

impl<TP> SeedCrawler<TP>
where
    TP: ProgressReporter,
{
    pub fn new(shutdown_notify: Arc<tokio::sync::Notify>, index: usize, seed: Url, progress_reporter: TP) -> Self {
        Self {
            shutdown_notify,
            index,
            seed,
            progress_reporter,
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn seed(&self) -> &Url {
        &self.seed
    }

    pub async fn crawl(&self, config: CrawlerConfig) -> anyhow::Result<CrawlSummary> {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        {
            let shutdown_notify = Arc::clone(&self.shutdown_notify);
            let shutdown_requested = Arc::clone(&shutdown_requested);
            tokio::task::spawn(async move {
                shutdown_notify.notified().await;
                shutdown_requested.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }

        self.progress_reporter.begin();

        let crawl_delay: Option<tokio::time::Duration> = {
            if let Some(requests_per_second) = config.requests_per_second {
                let crawl_delay_in_ms = (1000.0 / requests_per_second) as u64;
                Some(tokio::time::Duration::from_millis(crawl_delay_in_ms))
            } else {
                None
            }
        };

        let seed_url = self.seed.clone();
        let robots_txt_source = RobotsTxtSource::load_from_url(&seed_url, "rusty-spider").await?;
        let robots_txt_view = robots_txt_source.view();
        let robots_txt_matcher = robots_txt_view.matcher();

        let mut crawl_context = CrawlContext::new();
        crawl_context.add_url_to_crawl(&seed_url);

        self.progress_reporter
            .crawler_state_changed(CrawlerState::Crawling);

        let mut crawl_summary = CrawlSummary::default();
        while !shutdown_requested.load(std::sync::atomic::Ordering::Relaxed) && !crawl_context.is_crawling_complete() {
            let crawl_progress = crawl_context.progress();
            self.progress_reporter
                .progress_update(crawl_progress.0, crawl_progress.1);

            let output = self
                .crawl_next_url(&robots_txt_matcher, &mut crawl_context)
                .await?;
            let page_summary = match output {
                PageCrawlOutput::Success(page_summary) => Some(page_summary),
                PageCrawlOutput::HttpNotFound(url) => Some(PageSummary::from_status_code(url, 404)),
                PageCrawlOutput::HttpError(url, status_code) => {
                    Some(PageSummary::from_status_code(url, status_code))
                }
                PageCrawlOutput::NoMoreUrlsToCrawl => {
                    None
                },
                PageCrawlOutput::DeniedByRobotsTxt(url) => {
                    Some(PageSummary::from_status_code(url, 403))
                }
            };
            if let Some(page_summary) = page_summary {
                crawl_summary.add_page_summary(page_summary);
            }

            if let Some(crawl_delay) = crawl_delay {
                if !crawl_context.is_crawling_complete() {
                    if shutdown_requested.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }

                    self.progress_reporter
                        .crawler_state_changed(CrawlerState::Paused);
                    tokio::time::sleep(crawl_delay).await;
                    self.progress_reporter
                        .crawler_state_changed(CrawlerState::Crawling);
                }
            }
        }

        self.progress_reporter.end();

        Ok(crawl_summary)
    }

    async fn crawl_next_url(
        &self,
        robots_txt_matcher: &RobotsTxtMatcher<'_>,
        crawl_context: &mut CrawlContext,
    ) -> anyhow::Result<PageCrawlOutput> {
        // Fetch the next URL to crawl
        let url_to_crawl = crawl_context.pop_url_to_crawl();
        if let None = url_to_crawl {
            return Ok(PageCrawlOutput::NoMoreUrlsToCrawl);
        }
        let url_to_crawl = url_to_crawl.unwrap();
        crawl_context.mark_url_as_crawled(&url_to_crawl);

        // Ensure this URL is allowed to be crawled by robots.txt
        if !robots_txt_matcher.check_path(url_to_crawl.path()) {
            return Ok(PageCrawlOutput::DeniedByRobotsTxt(url_to_crawl));
        }

        {
            let msg = format!("Crawling {}", url_to_crawl);
            self.progress_reporter.progress_message(&msg);
        }

        // Fetch the contents of the URL
        let crawl_request = CrawlRequest {
            url: url_to_crawl.clone(),
        };
        let crawl_response = self.crawl_single_url(crawl_request).await;
        match crawl_response {
            Ok(crawl_response) => {
                crawl_context.add_urls_to_crawl(&crawl_response.internal_links);

                let page_summary = PageSummary::new(
                    crawl_response.url,
                    crawl_response.status_code,
                    crawl_response.content_type,
                    crawl_response.title,
                    crawl_response.outgoing_links.len(),
                );
                Ok(PageCrawlOutput::Success(page_summary))
            }
            Err(e) => match e {
                CrawlError::HttpError(status_code) => {
                    if status_code == 404 {
                        Ok(PageCrawlOutput::HttpNotFound(url_to_crawl))
                    } else {
                        Ok(PageCrawlOutput::HttpError(url_to_crawl, status_code))
                    }
                }
                _ => Err(anyhow::anyhow!("Crawl error: {}", e)),
            },
        }
    }

    async fn crawl_single_url(
        &self,
        crawl_request: CrawlRequest,
    ) -> Result<CrawlResponse, CrawlError> {
        let url_to_crawl = &crawl_request.url;

        let crawl_response = reqwest::get(url_to_crawl.clone()).await?;
        if !crawl_response.status().is_success() {
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
}
