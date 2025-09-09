use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

// Generic Error type for all errors in handlers
#[derive(Debug)]
pub enum Error {
    QueueEnabledOutOfRange(String),
    StoreCapacityOutOfRange(String),
    QueueSyncTimestampOutOfRange(String),
    RedisScriptUnreadable(String),
    Unknown(anyhow::Error),
}

// Generic Result type for all results in handlers
pub type Result<T> = core::result::Result<T, Error>;

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::QueueEnabledOutOfRange(size) => error!("queue enabled out of range: {}", size),
            Error::StoreCapacityOutOfRange(size) => error!("store capacity out of range: {}", size),
            Error::QueueSyncTimestampOutOfRange(size) => {
                error!("queue sync timestamp out of range: {}", size)
            }
            Error::RedisScriptUnreadable(script) => error!("script unreadable: {}", script),
            Error::Unknown(error) => error!("unknown error: {:?}", error),
        };

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal server error".to_string(),
        )
            .into_response()
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
                    .expect("Request builder failed"),
            )
            .await
            .expect("Axum app build failed");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let html = String::from_utf8(bytes.to_vec()).unwrap();

        assert_eq!(html, "internal server error");
    }
}
