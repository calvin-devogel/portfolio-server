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