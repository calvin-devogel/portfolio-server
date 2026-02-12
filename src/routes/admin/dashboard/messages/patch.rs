use actix_web::HttpResponse;

#[tracing::instrument(name = "Update messages")]
pub async fn patch_messages() -> HttpResponse {
    HttpResponse::Ok().finish()
}