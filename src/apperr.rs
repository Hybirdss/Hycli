//! hycli 오류 계약 — 세계일관 exit code + remedy(다음 행동 힌트).
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Ok,
    Runtime,
    Usage,
    Auth,
    Blocked,
    Spec,
}

impl Code {
    pub fn as_i32(self) -> i32 {
        match self {
            Code::Ok => 0,
            Code::Runtime => 1,
            Code::Usage => 2,
            Code::Auth => 3,
            Code::Blocked => 4,
            Code::Spec => 5,
        }
    }
}

#[derive(Debug)]
pub struct AppError {
    pub code: Code,
    pub msg: String,
    pub remedy: String,
    pub public_code: String,
    pub http_status: u16,
}

impl AppError {
    pub fn new(code: Code, msg: impl Into<String>, remedy: impl Into<String>) -> Self {
        let (public_code, http_status) = match code {
            Code::Ok => ("ok", 200),
            Code::Runtime => ("internal", 500),
            Code::Usage => ("bad_request", 400),
            Code::Auth => ("auth_required", 401),
            Code::Blocked => ("blocked_by_site", 429),
            Code::Spec => ("data_invalid", 400),
        };
        Self {
            code,
            msg: msg.into(),
            remedy: remedy.into(),
            public_code: public_code.into(),
            http_status,
        }
    }

    pub fn api(public_code: &str, http_status: u16) -> Self {
        let code = match http_status {
            400 | 404 | 409 | 413 => Code::Usage,
            401 => Code::Auth,
            403 | 429 => Code::Blocked,
            _ => Code::Runtime,
        };
        Self {
            code,
            msg: public_code.into(),
            remedy: match public_code {
                "auth_required" | "identity_unavailable" => "Open `hycli dashboard` and reconnect the website account or configure its website API key.",
                "provider_missing" | "provider_not_connected" | "provider_required" => "Open `hycli dashboard` and connect an AI provider in AI connections.",
                "query_failed" => "The website reported query errors. Check the inputs and the action definition before retrying.",
                "response_mismatch" => "The website returned a different response than this action expects. Reconnect if signed out, or prepare the website again.",
                "site_rate_limited" | "rate_limited" => "Wait for the website's request limit to clear before retrying.",
                "blocked_by_site" => "Check the website in your browser and resolve any sign-in or challenge before reconnecting.",
                "approval_required" | "approval_expired" => "Run the action again and review its exact inputs in the Hycli dashboard.",
                "action_outcome_unknown" => "Check the website before trying again; the change may already have happened.",
                "unsafe_target" | "local_target" => "Check the destination. For a trusted local website, use --allow-local explicitly.",
                "cancelled" | "interrupted" => "Review the activity entry in the dashboard before retrying.",
                "request_failed" | "timeout" => "Check the website and your connection, then retry the read or prepare the website again.",
                "no_actions" => "Open the website in your browser and connect its account, then retry preparation with the available evidence.",
                _ => "",
            }.into(),
            public_code: public_code.into(),
            http_status,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)?;
        if !self.remedy.is_empty() {
            write!(f, " — remedy: {}", self.remedy)?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        runtime(
            error,
            "Local database operation failed",
            "Check the data directory and try again.",
        )
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        runtime(
            error,
            "Local file operation failed",
            "Check the file and its permissions.",
        )
    }
}

impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::api("data_invalid", 400)
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::from_u16(self.http_status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        (
            status,
            axum::Json(serde_json::json!({"error":{"code":self.public_code}})),
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// 임의 오류를 Runtime 계약으로 감싼다.
pub fn runtime<E: std::error::Error>(
    err: E,
    msg: impl Into<String>,
    remedy: impl Into<String>,
) -> AppError {
    AppError::new(Code::Runtime, format!("{} ({})", msg.into(), err), remedy)
}

pub fn code_of(err: &AppError) -> Code {
    err.code
}

pub fn spec(msg: impl Into<String>, remedy: impl Into<String>) -> AppError {
    AppError::new(Code::Spec, msg, remedy)
}

pub fn usage(msg: impl Into<String>, remedy: impl Into<String>) -> AppError {
    AppError::new(Code::Usage, msg, remedy)
}

pub fn runtime_msg(msg: impl Into<String>, remedy: impl Into<String>) -> AppError {
    AppError::new(Code::Runtime, msg, remedy)
}

pub fn auth_err(msg: impl Into<String>, remedy: impl Into<String>) -> AppError {
    AppError::new(Code::Auth, msg, remedy)
}

pub fn blocked(msg: impl Into<String>, remedy: impl Into<String>) -> AppError {
    AppError::new(Code::Blocked, msg, remedy)
}
