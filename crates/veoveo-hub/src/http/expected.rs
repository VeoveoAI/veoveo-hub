//! Server errors the Hub answers on purpose. A response for a state the Hub reports and
//! logs itself, such as Computers being unavailable while the installation's template is
//! missing, is not a failure of the request: the trace layer does not log it as one.
use axum::response::Response;
use tower_http::classify::{
    ClassifiedResponse, ClassifyResponse, NeverClassifyEos, ServerErrorsAsFailures,
    ServerErrorsFailureClass,
};

/// Marks a server error response as expected.
#[derive(Debug, Clone, Copy)]
pub struct ExpectedServerError;

/// `response`, marked as an expected server error.
pub fn expected(mut response: Response) -> Response {
    response.extensions_mut().insert(ExpectedServerError);
    response
}

/// tower-http's server-error classification, except for responses marked
/// [`ExpectedServerError`].
#[derive(Debug, Clone, Copy, Default)]
pub struct UnexpectedServerErrors;

impl ClassifyResponse for UnexpectedServerErrors {
    type FailureClass = ServerErrorsFailureClass;
    type ClassifyEos = NeverClassifyEos<ServerErrorsFailureClass>;

    fn classify_response<B>(
        self,
        response: &axum::http::Response<B>,
    ) -> ClassifiedResponse<Self::FailureClass, Self::ClassifyEos> {
        if response.extensions().get::<ExpectedServerError>().is_some() {
            return ClassifiedResponse::Ready(Ok(()));
        }
        ServerErrorsAsFailures::new().classify_response(response)
    }

    fn classify_error<E>(self, error: &E) -> Self::FailureClass
    where
        E: std::fmt::Display + 'static,
    {
        ServerErrorsAsFailures::new().classify_error(error)
    }
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, response::IntoResponse};
    use tower_http::classify::{ClassifiedResponse, ClassifyResponse};

    use super::*;

    #[test]
    fn only_a_marked_server_error_is_no_failure() {
        let failed = |response: &Response| {
            matches!(
                UnexpectedServerErrors.classify_response(response),
                ClassifiedResponse::Ready(Err(_))
            )
        };
        assert!(failed(&StatusCode::SERVICE_UNAVAILABLE.into_response()));
        assert!(!failed(&expected(
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        )));
        assert!(!failed(&StatusCode::NOT_FOUND.into_response()));
    }
}
