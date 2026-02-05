use actix_web::HttpResponse;

#[tracing::instrument(name = "Get messages", skip_all)]
pub async fn get_messages() -> HttpResponse {
    HttpResponse::Ok().finish()
}