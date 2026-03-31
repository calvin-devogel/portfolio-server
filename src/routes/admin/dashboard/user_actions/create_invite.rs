use crate::{authentication::UserId, idempotency::execute_idempotent, types::user::CreateUser};
use actix_web::{HttpRequest, HttpResponse, web};
use rand::{RngExt, distr::Alphanumeric};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

#[tracing::instrument(name = "Create user invitation", skip_all)]
pub async fn create_user(
    new_user: web::Form<CreateUser>,
    pool: web::Data<PgPool>,
    request: HttpRequest,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_to_create = new_user.into_inner();
    let user_id = Some(**user_id);
    user_to_create.validate()?;

    execute_idempotent(&request, &pool, user_id, move |tx| {
        Box::pin(async move { process_create_new_user(tx, user_to_create).await })
    })
    .await
}

#[allow(clippy::future_not_send)]
async fn process_create_new_user(
    transaction: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    new_user: CreateUser,
) -> Result<HttpResponse, actix_web::Error> {
    // random raw token
    let raw_token: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    // hash the token
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let invitation_id = uuid::Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO user_invitations (id, email, role, invitation_token_hash, expires_at, created_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        "#,
        invitation_id,
        new_user.email,
        "user".to_string(), // default to "user" role for invitations, admin can change later
        token_hash.to_string(),
        expires_at,
    )
    .execute(transaction.as_mut())
    .await
    .map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create user invitation: {}", e))
    })?;

    // sidestepping an email service, don't really wanna implement that for this project
    let response_data = serde_json::json!({
        "success": true,
        "message": "Invitation created successfully.",
        "link": format!("http://localhost:4200/invitation/accept?token={}", raw_token)
    });

    Ok(HttpResponse::Ok().json(response_data))
}
