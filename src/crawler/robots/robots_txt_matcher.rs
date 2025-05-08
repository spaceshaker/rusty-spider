#[derive(Clone)]
pub struct RobotsTxtMatcher<'a> {
    matcher: robots_txt::matcher::SimpleMatcher<'a>,
}

impl<'a> RobotsTxtMatcher<'a> {
    pub fn new(matcher: robots_txt::matcher::SimpleMatcher<'a>) -> Self {
        Self { matcher }
    }

    pub fn check_path(&self, path: &str) -> bool {
        self.matcher.check_path(path)
    }
}