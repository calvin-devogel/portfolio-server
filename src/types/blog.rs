use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CarouselImage {
    src: String,
    alt: Option<String>,
    caption: Option<String>,
}


// need to parse something like `[order: 1]/[order: ...]`
// at the start of each section (this is a client-side thing)
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ArticleSection {
    Markdown { order: i8, content: String },
    Carousel { order: i8, images: Vec<CarouselImage> },
}

#[derive(serde::Serialize)]
pub struct ArticleRecord {
    post_id: Uuid,
    title: String,
    slug: String,
    excerpt: String,
    content: Vec<ArticleSection>,
    author: String,
    published: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(serde::Deserialize)]
pub struct ArticleForm {
    title: String,
    excerpt: String,
    sections: Vec<ArticleSection>,
    author: String,
}

#[derive(serde::Deserialize)]
pub struct ArticleDeleteRequest {
    post_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct ArticlePublishRequest {
    post_id: Uuid,
    published: bool,
}

#[derive(serde::Deserialize)]
pub struct ArticleEditRequest {
    post_id: Uuid,
    title: Option<String>,
    sections: Vec<ArticleSection>,
    excerpt: Option<String>,
    author: Option<String>,
}