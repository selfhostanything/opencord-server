use axum::extract::State;
use axum::response::IntoResponse;

use crate::state::AppState;

pub async fn prometheus(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.render_prometheus(),
    )
}
