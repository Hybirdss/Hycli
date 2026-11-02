//! Shared HTTP outcome classification, independent of transport and caller.
use crate::apperr::{AppError, AppResult, SafeHttpDiagnostic};

pub(super) fn ensure_success(
    status: u16,
    defense: &str,
    signal: &str,
    diagnostic: Option<SafeHttpDiagnostic>,
) -> AppResult<()> {
    let (code, outward_status) = match signal {
        "query_failed" => ("query_failed", 502),
        "action_outcome_unknown" => ("action_outcome_unknown", 502),
        _ => match status {
            401 => ("auth_required", 401),
            429 => ("site_rate_limited", 429),
            _ if defense != "none" && !defense.is_empty() => ("blocked_by_site", 403),
            403 => ("api_denied", 403),
            200..300 => return Ok(()),
            _ => ("request_failed", 502),
        },
    };
    let mut error = AppError::api(code, outward_status);
    error.diagnostic = diagnostic;
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_outcomes_preserve_precedence_and_status() {
        for (status, defense, signal, expected, outward) in [
            (403, "none", "", "api_denied", 403),
            (403, "captcha", "", "blocked_by_site", 403),
            (200, "captcha", "", "blocked_by_site", 403),
            (401, "captcha", "", "auth_required", 401),
            (429, "none", "", "site_rate_limited", 429),
            (403, "none", "query_failed", "query_failed", 502),
            (
                403,
                "none",
                "action_outcome_unknown",
                "action_outcome_unknown",
                502,
            ),
            (404, "none", "", "request_failed", 502),
            (500, "none", "", "request_failed", 502),
        ] {
            let diagnostic = SafeHttpDiagnostic::from_response(status, defense, None, &[]);
            let error = ensure_success(status, defense, signal, Some(diagnostic)).unwrap_err();
            assert_eq!(error.public_code, expected);
            assert_eq!(error.http_status, outward);
            assert_ne!(error.code.as_i32(), 0);
            assert_eq!(
                error.public_error()["error"]["diagnostic"]["upstream_status"],
                status
            );
        }
        assert!(ensure_success(200, "none", "", None).is_ok());
        assert!(ensure_success(204, "", "", None).is_ok());
    }
}
