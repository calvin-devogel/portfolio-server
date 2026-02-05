use actix_web::HttpResponse;

#[tracing::instrument(name = "Send message to contact table", skip_all)]
pub async fn post_message() -> HttpResponse {
    HttpResponse::Ok().finish()
}