use actix_web::{HttpResponse};

pub async fn test_reject() -> Result<HttpResponse, actix_web::Error> {
    Ok(HttpResponse::Ok().finish())
}