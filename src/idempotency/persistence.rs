use crate::errors::IdempotencyError;

use super::IdempotencyKey;
use actix_web::{HttpResponse, HttpRequest, body::to_bytes, http::StatusCode};
use std::future::Future;
use std::pin::Pin;
use sqlx::{Executor, PgPool, Postgres, Transaction};
use uuid::Uuid;

// header pair type for sqlx
#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "header_pair")]
struct HeaderPairRecord {
    name: String,
    value: Vec<u8>,
}

// determines what to do with the incoming request
#[allow(clippy::large_enum_variant)]
pub enum NextAction {
    // first time seeing this request, proceed without holding the transaction
    // transactions are not send-safe so we need to consume them immediately
    StartProcessing,
    // already processed, return the cached response
    ReturnSavedResponse(HttpResponse),
}

// tries to insert a new row with key + user_id (this will need to change)
// if the row is able to be inserted -> StartProcessing a transaction
// if the row already exists -> fetch saved response and return it
pub async fn try_processing(
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    user_id: Option<Uuid>,
) -> Result<(NextAction, Option<Transaction<'static, Postgres>>), IdempotencyError> {
    let mut transaction = pool.begin().await?;
    let query = sqlx::query!(
        r#"
        INSERT INTO idempotency (
            user_id,
            idempotency_key,
            created_at
        )
        VALUES ($1, $2, now())
        ON CONFLICT DO NOTHING
        "#,
        user_id, // can be NULL now
        idempotency_key.as_ref()
    );
    let n_inserted_rows = transaction.execute(query).await?.rows_affected();
    if n_inserted_rows > 0 {
        Ok((NextAction::StartProcessing, Some(transaction)))
    } else {
        let saved_response = get_saved_response(pool, idempotency_key, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("We expected a saved response, we didn't find it"))?;
        Ok((NextAction::ReturnSavedResponse(saved_response), None))
    }
}

// deconstruct response into head + body
// converts the body to bytes (since response streams can't be replayed)
// stores status code, headers, and body in the database
// commits the transaction
// returns HttpResponse
#[allow(clippy::future_not_send)]
pub async fn save_response(
    mut transaction: Transaction<'static, Postgres>,
    idempotency_key: &IdempotencyKey,
    user_id: Option<Uuid>,
    http_response: HttpResponse,
) -> Result<HttpResponse, IdempotencyError> {
    let (response_head, body) = http_response.into_parts();
    // MessageBody::Error is not `Send` + `Sync`
    // -> it does not play nicely with `anyhow`
    let body = to_bytes(body).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let status_code = response_head.status().as_u16().cast_signed();
    let headers = {
        let mut h = Vec::with_capacity(response_head.headers().len());
        for (name, value) in response_head.headers() {
            let name = name.as_str().to_owned();
            let value = value.as_bytes().to_owned();
            h.push(HeaderPairRecord { name, value });
        }
        h
    };

    transaction
        .execute(sqlx::query_unchecked!(
            r#"
                UPDATE idempotency 
                SET
                    response_status_code = $3,
                    response_headers = $4,
                    response_body = $5
                WHERE
                    idempotency_key = $2
                    AND (user_id = $1 OR (user_id IS NULL AND $1 IS NULL))
                "#,
            user_id,
            idempotency_key.as_ref(),
            status_code,
            headers,
            body.as_ref()
        ))
        .await?;
    transaction.commit().await?;
    // we need `.map_into_boxed_body` to go from `HttpResponse<Bytes>` to `HttpResponse<BoxBody`
    // pulling a chunk of data from the payload stream requires a mutable reference to the stream itself
    // once the chunk has been read, there is no way to "replay" the stream and read it again
    // common pattern to work around this:
    // - get ownership of the body via .into_parts();
    // - buffer the whole body in memory via to_bytes;
    // - do whatever you have to do with the body
    // - re-assemble the response using .set_body() on the request head
    let http_response = response_head.set_body(body).map_into_boxed_body();
    Ok(http_response)
}

// queries the database for saved Response data
// reconstructs the HttpResponse for saved response data
// returns `None` if not found
pub async fn get_saved_response(
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    user_id: Option<Uuid>,
) -> Result<Option<HttpResponse>, anyhow::Error> {
    let saved_response = sqlx::query!(
        r#"
        SELECT
            response_status_code as "response_status_code!",
            response_headers as "response_headers!: Vec<HeaderPairRecord>",
            response_body as "response_body!"
        FROM idempotency
        WHERE
            idempotency_key = $2
            AND (user_id = $1 OR (user_id IS NULL AND $1 IS NULL))
        "#,
        user_id,
        idempotency_key.as_ref()
    )
    .fetch_optional(pool)
    .await?;
    if let Some(r) = saved_response {
        let status_code = StatusCode::from_u16(r.response_status_code.try_into()?)?;
        let mut response = HttpResponse::build(status_code);
        for HeaderPairRecord { name, value } in r.response_headers {
            response.append_header((name, value));
        }
        Ok(Some(response.body(r.response_body)))
    } else {
        Ok(None)
    }
}

// Request arrives -> `try_processing()` checks if it's been seen before
// if new -> process the request -> cache result with `save_response()`
// if duplicate -> `get_saved_response()` returns the cached result immediately

// there are a few places where an idempotency key is required, use this wherever it is
pub fn get_idempotency_key(request: HttpRequest) -> Result<IdempotencyKey, IdempotencyError> {
    let idempotency_key: IdempotencyKey = request
        .headers()
        .get("Idempotency-Key")
        .and_then(|header| header.to_str().ok())
        .ok_or_else(|| {
            tracing::warn!("Missing Idempotency-Key header");
            IdempotencyError::MissingIdempotencyKey
        })?
        .to_string()
        .try_into()
        .map_err(|e| {
            tracing::warn!(error = ?e, "Invalid idempotency key format");
            IdempotencyError::InvalidKeyFormat
        })?;

    Ok(idempotency_key)
}

// reusable idempotency flow for all handlers that need it
pub async fn execute_idempotent<F, E>(
    request: &HttpRequest,
    pool: &PgPool,
    user_id: Option<Uuid>,
    operation: F
) -> Result<HttpResponse, E>
where
    F: for<'a> FnOnce(
        &'a mut Transaction<'static, Postgres>,
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, E>> + 'a>>,
    E: From<IdempotencyError>,
{
    let key = get_idempotency_key(request.clone()).map_err(E::from)?;
    let (action, tx_opt) = try_processing(pool, &key, user_id).await.map_err(E::from)?;

    match (action, tx_opt) {
        (NextAction::ReturnSavedResponse(saved_response), _) => Ok(saved_response),

        (NextAction::StartProcessing, Some(mut tx)) => {
            // wrap all this in tx
            let response = operation(&mut tx).await?;
            let response = save_response(tx, &key, user_id, response)
                .await
                .map_err(E::from)?;
            Ok(response)
        }

        (NextAction::StartProcessing, None) => {
            Err(E::from(IdempotencyError::UnexpectedError(anyhow::anyhow!(
                "Missing transaction for StartProcessing"
            ))))
        }
    }
}
