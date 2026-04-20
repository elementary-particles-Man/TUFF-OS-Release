use axum::http::StatusCode;

pub const ADD_TASK_TCP_DISABLED: &str = "tcp_add_task_disabled_use_ipc";

pub async fn add_task() -> (StatusCode, &'static str) {
    (StatusCode::GONE, ADD_TASK_TCP_DISABLED)
}
