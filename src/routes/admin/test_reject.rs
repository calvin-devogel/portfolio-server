use actix_web::HttpResponse;

#[allow(clippy::missing_errors_doc)]
pub async fn test_reject() -> Result<HttpResponse, actix_web::Error> {
    Ok(HttpResponse::Ok().finish())
}
