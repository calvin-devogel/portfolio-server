use actix_web::{HttpResponse, http::StatusCode};

#[derive(serde::Serialize, Debug)]
pub struct ApiErrorResponse {
    pub code: &'static str,
    pub message: String,
}

pub trait AppError: std::error::Error + 'static {
    fn code(&self) -> &'static str;
    fn client_message(&self) -> &str;
    fn http_status(&self) -> StatusCode;
    fn override_message(&self) -> Option<&str> {
        None
    }
}

pub fn build_error_response(e: &impl AppError) -> HttpResponse {
    let status = e.http_status();

    if status.is_server_error() {
        tracing::error!(
            error.code = e.code(),
            error.detail = %e,
            error.source = ?std::error::Error::source(e),
            "Internal server error"
        );
    } else {
        tracing::warn!(
            error.code = e.code(),
            error.detail = %e,
            "Client error"
        );
    }

    HttpResponse::build(status).json(ApiErrorResponse {
        code: e.code(),
        message: e
            .override_message()
            .unwrap_or_else(|| e.client_message())
            .to_owned(),
    })
}

use actix_web::http::header::LOCATION;

// redirect (don't think I need this on the server side, probably have to send a signal?)
#[must_use]
pub fn see_other(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, location))
        .finish()
}

// format the error chain
pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{e}\n")?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{cause}")?;
        current = cause.source();
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use actix_web::http::StatusCode;
    use std::fmt;

    #[test]
    fn see_other_returns_303_with_location_header() {
        let response = see_other("/new-location");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get(LOCATION).unwrap(), "/new-location");
    }

    // minimal single-level error
    #[derive(Debug)]
    struct LeafError;

    impl fmt::Display for LeafError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Leaf error")
        }
    }

    impl std::error::Error for LeafError {}

    // multi-level error that chains to LeafError
    #[derive(Debug)]
    struct WrapperError(LeafError);

    impl fmt::Display for WrapperError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Wrapper error: {}", self.0)
        }
    }

    impl std::error::Error for WrapperError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    struct ChainDisplay<'a>(&'a dyn std::error::Error);

    impl<'a> fmt::Display for ChainDisplay<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            error_chain_fmt(&self.0, f)
        }
    }

    #[test]
    fn error_chain_fmt_single_error_no_cause() {
        let e = ChainDisplay(&LeafError);
        assert!(format!("{}", e).contains("Leaf error"));
        assert!(!format!("{}", e).contains("Caused by:"));
    }

    #[test]
    fn error_chain_fmt_multiple_errors_with_causes() {
        let e = ChainDisplay(&WrapperError(LeafError));
        let output = format!("{}", e);
        assert!(output.contains("Wrapper error: Leaf error"));
        assert!(output.contains("Caused by:\n\tLeaf error"));
    }
}
