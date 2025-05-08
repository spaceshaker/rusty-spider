use url::Url;
use reqwest::StatusCode;
use robots_txt::Robots;
use crate::crawler::robots::robots_txt_view::RobotsTxtView;

#[derive(Clone)]
pub struct RobotsTxtSource {
    content: String,
    agent: String,
}

impl RobotsTxtSource {
    pub async fn load_from_url(url: &Url, agent: &str) -> anyhow::Result<Self> {
        let mut robots_txt_url = url.clone();
        robots_txt_url.set_path("/robots.txt");
        let robots_response = reqwest::get(robots_txt_url).await?;
        if !robots_response.status().is_success() {
            if robots_response.status() == StatusCode::NOT_FOUND {
                return Ok(Self {
                    content: String::new(),
                    agent: agent.to_owned(),
                });
            }
            return Err(anyhow::anyhow!("An error occurred fetching robots.txt"));
        }
        let content = robots_response.text().await?;
        Ok(Self {
            content,
            agent: agent.to_owned(),
        })
    }

    pub fn view(&self) -> RobotsTxtView<'_> {
        let context = self.content.as_str();
        let robot = Robots::from_str_lossy(context);
        RobotsTxtView::new(context, robot, self.agent.clone())
    }
}