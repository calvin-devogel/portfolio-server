use actix_web::{HttpResponse, http::header::LOCATION};

// http 400 aka client-side error
pub fn e400<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorBadRequest(e)
}

// http 500 aka server-side error
pub fn e500<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}

// redirect (don't think I need this on the server side, probably have to send a signal?)
#[must_use]
pub fn see_other(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, location))
        .finish()
}

#[must_use]
pub fn unauthorized() -> HttpResponse {
    HttpResponse::Unauthorized().finish()
}

// format the error chain
#[allow(clippy::missing_errors_doc)]
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
    fn e400_returns_bad_request() {
        let err = e400("bad input");
        assert_eq!(
            err.as_response_error().status_code(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn e500_returns_internal_server_error() {
        let err = e500("something went wrong");
        assert_eq!(
            err.as_response_error().status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn see_other_returns_303_with_location_header() {
        let response = see_other("/new-location");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get(LOCATION).unwrap(), "/new-location");
    }

    #[test]
    fn unauthorized_returns_401() {
        let response = unauthorized();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
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
