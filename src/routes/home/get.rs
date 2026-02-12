use actix_web::{HttpResponse, http::header::ContentType};

pub async fn root() -> HttpResponse {
    let html = include_str!("./index.html");

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(html)
}