use crate::errors::IdempotencyError;

use super::IdempotencyKey;
use actix_web::{HttpRequest, HttpResponse, body::to_bytes, http::StatusCode};
use sqlx::{Executor, PgPool, Postgres, Transaction};
use std::future::Future;
use std::pin::Pin;
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
/// as for why (NextAction::StartProcessing, None) is an unreachable state:
///     - if n_inserted_rows > 0, return (NextAction::StartProcessing, Some(transaction))
///     - if n_inserted_rows == 0, return *either*
///         - (NextAction::ReturnSavedResponse(response), None) or
///         - (IdempotencyError::RequestInFlight)
/// so no path allows the match statement to find (NextAction, None)
pub async fn try_processing(
    pool: &PgPool,
    idempotency_key: &IdempotencyKey,
    user_id: Option<Uuid>,
    operation: &str,
) -> Result<(NextAction, Option<Transaction<'static, Postgres>>), IdempotencyError> {
    let mut transaction = pool.begin().await?;
    let query = sqlx::query!(
        r#"
        INSERT INTO idempotency (
            user_id,
            idempotency_key,
            operation,
            created_at
        )
        VALUES ($1, $2, $3, now())
        ON CONFLICT DO NOTHING
        "#,
        user_id, // can be NULL now
        idempotency_key.as_ref(),
        operation
    );
    let n_inserted_rows = transaction.execute(query).await?.rows_affected();
    if n_inserted_rows > 0 {
        Ok((NextAction::StartProcessing, Some(transaction)))
    } else {
        let saved_response = get_saved_response(pool, idempotency_key, user_id, operation).await?;

        saved_response.map_or_else(
            || Err(IdempotencyError::RequestInFlight),
            |response| Ok((NextAction::ReturnSavedResponse(response), None)),
        )
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
    operation: &str,
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
                    AND operation = $6
                    AND (user_id = $1 OR (user_id IS NULL AND $1 IS NULL))
                "#,
            user_id,
            idempotency_key.as_ref(),
            status_code,
            headers,
            body.as_ref(),
            operation
        ))
        .await?;
    transaction.commit().await?;
    // we need `.map_into_boxed_body` to go from `HttpResponse<Bytes>` to `HttpResponse<BoxBody>`
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
    operation: &str,
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
            AND operation = $3
            AND response_status_code IS NOT NULL
            AND (user_id = $1 OR (user_id IS NULL AND $1 IS NULL))
        "#,
        user_id,
        idempotency_key.as_ref(),
        operation
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
pub fn get_idempotency_key(request: &HttpRequest) -> Result<IdempotencyKey, IdempotencyError> {
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

// wrapper for execute_idempotent_with that calls the default process_fn
// (try_processing) that all non-test callers will use.
pub async fn execute_idempotent<F, E>(
    request: &HttpRequest,
    pool: &PgPool,
    user_id: Option<Uuid>,
    action: F,
) -> Result<HttpResponse, E>
where
    F: for<'a> FnOnce(
        &'a mut Transaction<'static, Postgres>,
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, E>> + 'a>>,
    E: From<IdempotencyError> + std::fmt::Debug,
{
    execute_idempotent_with(request, pool, user_id, action, |pool, key, user_id, op| {
        Box::pin(async move {
            try_processing(pool, key, user_id, op)
                .await
                .map_err(|e| E::from(e))
        })
    })
    .await
}

/// jesus what a mess
/// Here's what's happening in this signature: since there are multiple different endpoints that require
/// idempotency, the flow for idempotent actions needs to be both reusable, and able to accept
/// generic actions (`action: F`), and in order to test all paths in the match statement, we need to be
/// able to mock process_fn in a way that allows us to return NextAction::StartProcessing
/// accompanied by a missing transaction operation, since otherwise, that state is unreachable.
/// (see try_processing for why that's a practically unreachable state)
/// So, from top to bottom: execute_idempotent_with is a function that takes as parameters:
///     - a reference to an HTTP request (this is where the actual data inserted/edited/etc. comes from)
///     - a reference to the Postgres connection pool
///     - an optional user_id depending on whether or not the action is anonymous
///     - an arbitrary `action: F` that:
///         - is valid for all possible lifetimes 'a
///         - is executed a single time inside the idempotency pipeline
///         - returns a pinned, safe-to-poll pointer (valid for 'a) to a dynamically-dispatched future, the output of which:
///             - is a Result that on success, is an HTTP response
///             - and on error, is an arbitrary error E
///         - takes as its parameter: a mutable reference to the active Postgres transaction to
///           ensure that in addition to being idempotent, this action is also atomic
///     - a process function `process_fn: P` that:
///         - is valid for all possible lifetimes 'p
///         - is executed a single time inside the idempotency pipeline
///         - returns a pinned, safe-to-poll pointer (also valid for 'p) to a dynamically-dispatched future, the output of which:
///             - is a Result that on success, is a tuple containing a variant of NextAction and an "optional" Postgres transaction
///               This right here adds another layer of complexity in the form of the process function. ^
///               (in practice, this transaction will never be None, but because a hypothetical future refactor *could* make
///               that state reachable, the match statement needs to account for that, and therefore, so does our testing suite,
///               thus the mockability requirement)
///             - and on error, is an arbitrary error E (wrapped in IdempotencyError::UnexpectedError)
///         - and takes as parameters:
///             - a reference to the Postgres connection pool
///             - the idempotency key (a caller-provided user_id + operation-scoped idempotency key)
///             - an optional user_id (for authenticated/anonymous actions)
///             - and an operation identifier (ie. "POST:/api/contact")
/// and returns:
///     - a Result that on success, is the HTTP response returned by `action`
///     - and on error, is the generic error E from either `action`, `process` or itself, which must:
///         - be some variant of IdempotencyError, and implement std::fmt::Debug
///           (they all do, but Rust wants us to specify that anyway)
///
/// State machine: extract idempotency key from request header -> build operation key as `METHOD:PATH` -> ask process_fn what to do next
///     -> StartProcessing + Some(tx) -> run once, then persist response in the same tx
///     -> ReturnSavedResponse + _ -> return the cached response immediately
///     -> StartProcessing + None -> treat as an invariant violation
///
/// Problem: prevent business logic side effects on retried requests
/// Approach: claim idempotency slot, process once in transaction, cache full HTTP response
/// Rust-Specific Challenge: generic async closures with lifetimes and transactional ownership
/// Design Choice: injectable process_fn to keep flow reusable and ergonomically-tested
///
/// Trade-Offs:
/// - response body is fully buffered in memory before persistence (?)
/// - in-flight duplicates returns an error path rather than waiting for the first write completion
/// - operation scope must include METHOD:PATH to prevent key collisions
#[doc(hidden)]
#[allow(clippy::future_not_send)]
// reusable idempotency flow for all handlers that need it
pub async fn execute_idempotent_with<F, P, E>(
    request: &HttpRequest,
    pool: &PgPool,
    user_id: Option<Uuid>,
    action: F,
    process_fn: P,
) -> Result<HttpResponse, E>
where
    F: for<'a> FnOnce(
        &'a mut Transaction<'static, Postgres>,
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, E>> + 'a>>,
    P: for<'p> FnOnce(
        &'p PgPool,
        &'p IdempotencyKey,
        Option<Uuid>,
        &'p str, // operation identifier
    ) -> Pin<
        Box<
            dyn Future<Output = Result<(NextAction, Option<Transaction<'static, Postgres>>), E>>
                + 'p,
        >,
    >,
    E: From<IdempotencyError> + std::fmt::Debug,
{
    let key = get_idempotency_key(request).map_err(E::from)?;
    let operation = format!("{}:{}", request.method().as_str(), request.path());
    let (next, tx_opt) = process_fn(pool, &key, user_id, &operation)
        // propogate error directly from process_fn so we actually know what happened
        .await?;

    match (next, tx_opt) {
        (NextAction::ReturnSavedResponse(saved_response), _) => Ok(saved_response),

        (NextAction::StartProcessing, Some(mut tx)) => {
            // wrap all this in tx
            let response = action(&mut tx).await?;
            let response = save_response(tx, &key, user_id, &operation, response)
                .await
                .map_err(E::from)?;
            Ok(response)
        }

        (NextAction::StartProcessing, None) => Err(E::from(IdempotencyError::UnexpectedError(
            anyhow::anyhow!("Missing transaction for StartProcessing"),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn get_idempotency_key_valid() {
        let request = TestRequest::default()
            .insert_header(("Idempotency-Key", "valid_key"))
            .to_http_request();

        let key = get_idempotency_key(&request).unwrap();
        assert_eq!(key.as_ref(), "valid_key");
    }

    #[test]
    fn get_idempotency_key_missing() {
        let request = TestRequest::default().to_http_request();
        let result = get_idempotency_key(&request);
        assert!(result.is_err());
    }

    #[test]
    fn get_idempotency_key_invalid_format() {
        let request = TestRequest::default()
            .insert_header(("Idempotency-Key", ""))
            .to_http_request();
        let result = get_idempotency_key(&request);
        assert!(result.is_err());
    }
}
