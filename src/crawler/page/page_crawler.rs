use crate::crawler::crawl_error::CrawlError;
use crate::crawler::crawl_response::CrawlResponse;
use anyhow::anyhow;
use std::collections::HashSet;
use url::Url;

pub struct PageCrawler {}

impl PageCrawler {
    pub fn new() -> Self {
        Self {}
    }
    
    pub async fn crawl(&self, url: &Url) -> Result<CrawlResponse, CrawlError> {
        let url_to_crawl = url;

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