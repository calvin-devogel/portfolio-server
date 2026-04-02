use crate::types::user::{User, UserRole};
use actix_web::{HttpResponse, web};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn get_all_users(pool: web::Data<PgPool>) -> Result<HttpResponse, actix_web::Error> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT
            user_id::TEXT as "user_id!",
            username,
            role::TEXT as "role!",
            must_change_password
        FROM users"#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(users))
}

pub async fn get_username_by_id(
    pool: web::Data<PgPool>,
    user_id: Uuid,
) -> Result<String, actix_web::Error> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT
            user_id::TEXT as "user_id!",
            username,
            role::TEXT as "role!",
            must_change_password
        FROM users
        WHERE user_id = $1::UUID
        "#,
        user_id
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(user.username)
}

#[derive(serde::Deserialize)]
pub struct RoleUpdate {
    pub role: UserRole,
}

pub async fn set_user_role(
    pool: web::Data<PgPool>,
    user_id: web::Path<Uuid>,
    new_role: web::Json<RoleUpdate>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    let new_role = new_role.into_inner();

    sqlx::query!(
        r#"
        UPDATE users
        SET role = $1
        WHERE user_id = $2::UUID
        "#,
        new_role.role as UserRole,
        user_id,
    )
    .execute(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().finish())
}

pub async fn reset_password(
    pool: web::Data<PgPool>,
    user_id: web::Path<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    sqlx::query!(
        r#"
        UPDATE users
        SET must_change_password = TRUE
        WHERE user_id = $1::UUID
        "#,
        user_id,
    )
    .execute(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().finish())
}
