use url::Url;

#[derive(Debug, Clone)]
pub struct PageSummary {
    pub url: Url,
    pub status_code: u16,
    pub content_type: String,
    pub title: String,
    pub num_outgoing_links: usize,
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

    pub fn from_status_code(url: Url, status_code: u16) -> Self {
        Self {
            url,
            status_code,
            content_type: String::new(),
            title: String::new(),
            num_outgoing_links: 0,
        }
    }
}