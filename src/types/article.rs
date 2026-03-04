use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::ops::Deref;

use crate::errors::BlogError;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CarouselImage {
    pub src: String,
    pub alt: Option<String>,
    pub caption: Option<String>,
}

// need to parse something like `[order: 1]/[order: ...]`
// at the start of each section (this is a client-side thing)
// sike no you dont, JSON is ordered lol
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ArticleSection {
    Markdown { content: String },
    Carousel { label: String, slides: Vec<CarouselImage> },
}

#[derive(serde::Serialize)]
pub struct ArticleRecord {
    pub post_id: Uuid,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub sections: Vec<ArticleSection>,
    pub author: String,
    pub published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ArticleRecord {
    pub fn try_from_row(
        post_id: Uuid,
        title: String,
        slug: String,
        excerpt: String,
        sections_json: serde_json::Value,
        author: String,
        published: bool,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, serde_json::Error> {
        let sections: Vec<ArticleSection> = serde_json::from_value(sections_json)?;
        Ok(Self {
            post_id,
            title,
            slug,
            excerpt,
            sections,
            author,
            published,
            created_at,
            updated_at,
        })
    }
}

pub struct ArticleRecordRaw {
    pub post_id: Uuid,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub sections: serde_json::Value,
    pub author: String,
    pub published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<ArticleRecordRaw> for ArticleRecord {
    type Error = serde_json::Error;

    fn try_from(raw: ArticleRecordRaw) -> Result<Self, Self::Error> {
        ArticleRecord::try_from_row(
            raw.post_id, 
            raw.title, 
            raw.slug, 
            raw.excerpt, 
            raw.sections, 
            raw.author, 
            raw.published, 
            raw.created_at, 
            raw.updated_at
        )
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ArticleId(pub Uuid);

impl std::fmt::Display for ArticleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for ArticleId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(serde::Serialize)]
pub struct ArticleResponse {
    pub message: &'static str,
    pub post_id: ArticleId,
}

impl ArticleResponse {
    pub const fn new(message: &'static str, post_id: ArticleId) -> Self {
        Self { message, post_id }
    }
}

#[derive(serde::Deserialize)]
pub struct ArticleForm {
    pub title: String,
    pub excerpt: String,
    pub sections: Vec<ArticleSection>,
    pub author: String,
}

impl ArticleForm {
    pub fn validate(&self) -> Result<(), BlogError> {
        let fields = [
            ("title", &self.title),
            ("excerpt", &self.excerpt),
            ("author", &self.author),
        ];
        for (_name,value) in fields {
            if value.trim().is_empty() {
                return Err(BlogError::ValidationError("Failed to validate form content".into()));
            }
        }
        if self.sections.is_empty() {
            return Err(BlogError::ValidationError("Sections field is empty".into()));
        }
        Ok(())
    }

    pub fn sections_as_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(&self.sections)
    }
}

#[derive(serde::Deserialize)]
pub struct ArticleDeleteRequest {
    pub post_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct ArticlePublishRequest {
    pub post_id: Uuid,
    pub published: bool,
}

#[derive(serde::Deserialize)]
pub struct ArticleEditRequest {
    pub post_id: Uuid,
    pub title: Option<String>,
    pub sections: Option<Vec<ArticleSection>>,
    pub excerpt: Option<String>,
    pub author: Option<String>,
}

impl ArticleEditRequest {
    pub fn sections_as_json(&self) -> Result<Option<serde_json::Value>, serde_json::Error> {
        self.sections
            .as_ref()
            .map(|s| serde_json::to_value(s))
            .transpose()
    }
}