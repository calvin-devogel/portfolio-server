use serde::{Deserialize, Serialize};

fn default_page() -> i64 {
    1
}

fn default_page_size() -> i64 {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

impl PaginationQuery {
    pub fn page(&self) -> i64 {
        self.page.max(1)
    }

    pub fn page_size(&self) -> i64 {
        self.page_size.clamp(1,20)
    }

    pub fn limit(&self) -> i64 {
        self.page_size()
    }

    pub fn offset(&self) -> i64 {
        (self.page() - 1) * self.page_size()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginationMeta {
    pub page: i64,
    pub page_size: i64,
    pub total_items: i64,
    pub total_pages: i64,
}

impl PaginationMeta {
    pub fn from_total(total_items: i64, query: &PaginationQuery) -> Self {
        let page_size = query.page_size();
        let total_pages = if total_items == 0 {
            0
        } else {
            (total_items + page_size - 1) / page_size
        };

        Self {
            page: query.page(),
            page_size,
            total_items,
            total_pages,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}