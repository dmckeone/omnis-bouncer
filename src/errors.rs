use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

// Generic Error type for all errors in handlers
#[derive(Debug)]
pub enum Error {
    StoreCapacityOutOfRange(isize),
    RedisScriptUnreadable(String),
    Unknown(anyhow::Error),
}

// Generic Result type for all results in handlers
pub type Result<T> = core::result::Result<T, Error>;

fn store_capacity_out_of_range(size: isize) -> Response {
    tracing::error!("store capacity error: {}", size);

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error".to_string(),
    )
        .into_response()
}

fn redis_script_unreadable(error: String) -> Response {
    tracing::error!("script error: {}", error);

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error".to_string(),
    )
        .into_response()
}

fn internal_server_error(error: anyhow::Error) -> Response {
    tracing::error!("unknown error: {}", error);

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error".to_string(),
    )
        .into_response()
}

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::StoreCapacityOutOfRange(size) => store_capacity_out_of_range(size),
            Error::RedisScriptUnreadable(e) => redis_script_unreadable(e),
            Error::Unknown(e) => internal_server_error(e),
        }
    }
}

// Enable using `?` on functions that return `Result<_, Error>` to turn them into
// `Result<_, Error::UnknownError<anyhow::Error>>`. That way you don't need to do that manually.
impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Error::Unknown(err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::{body::Body, http::Request, http::StatusCode, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn try_thing() -> anyhow::Result<()> {
        anyhow::bail!("it failed!")
    }

    async fn handle_anyhow_error() -> Result<()> {
        try_thing()?;
        Ok(())
    }

    fn test_app() -> Router {
        Router::new().route("/anyhow_error", get(handle_anyhow_error))
    }

    #[tokio::test]
    async fn test_anyhow_error() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/anyhow_error")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let html = String::from_utf8(bytes.to_vec()).unwrap();

        assert_eq!(html, "internal server error");
    }
}
