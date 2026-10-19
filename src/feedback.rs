//! Feedback channel: compose a pre-filled GitHub issue URL and hand it to the user's browser.
//!
//! The CLI only builds a string. Opening the URL contacts GitHub with the version and platform;
//! the user reviews and submits the issue in their browser. The URL is always returned as data.

use url::Url;

/// Generic issue editor for feedback, separate from the structured bug-report form.
pub const ISSUES_URL: &str = "https://github.com/Hybirdss/Hycli/issues/new";

/// Version and platform, the two facts every report needs and users otherwise retype by hand.
pub fn environment_summary() -> String {
    format!(
        "hycli {} · {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Issue body with the environment already filled in and a reminder about private data.
pub fn issue_body() -> String {
    format!(
        "### What happened\n\n<!-- Describe the problem or the idea. Remove credentials, cookies and private website data. -->\n\n### Environment\n\n- {}\n",
        environment_summary()
    )
}

/// URL that opens the issue form with title and body pre-filled. Pure string assembly: no request.
pub fn issue_url() -> String {
    let title = format!("[Feedback] {}", environment_summary());
    let mut url = Url::parse(ISSUES_URL).expect("ISSUES_URL is a valid absolute URL");
    url.query_pairs_mut()
        .append_pair("title", &title)
        .append_pair("body", &issue_body());
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_url_targets_the_public_issue_form() {
        let url = Url::parse(&issue_url()).unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("github.com"));
        assert_eq!(url.path(), "/Hybirdss/Hycli/issues/new");
    }

    #[test]
    fn title_and_body_carry_the_environment_without_raw_newlines() {
        let url = issue_url();
        assert!(!url.contains('\n'), "newlines must be percent-encoded");
        let parsed = Url::parse(&url).unwrap();
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        let title = query.get("title").expect("title");
        let body = query.get("body").expect("body");
        assert!(title.contains(env!("CARGO_PKG_VERSION")), "{title}");
        assert!(title.contains(std::env::consts::OS), "{title}");
        assert!(body.contains(&environment_summary()), "{body}");
        assert!(body.contains("### Environment"), "{body}");
    }
}
