use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebVitalsPayload {
    pub metrics: Vec<WebVitalEntry>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WebVitalEntry {
    pub name: WebVitalName,
    pub value: f64,
    pub rating: WebVitalRating,
    pub pathname: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum WebVitalName {
    Lcp,
    Inp,
    Cls,
    Ttfb,
    Fcp,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum WebVitalRating {
    Good,
    NeedsImprovement,
    Poor,
}

impl std::fmt::Display for WebVitalRating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebVitalRating::Good => write!(f, "good"),
            WebVitalRating::NeedsImprovement => write!(f, "needs-improvement"),
            WebVitalRating::Poor => write!(f, "poor"),
        }
    }
}