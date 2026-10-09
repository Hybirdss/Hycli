//! 방어 분류기 — 사이트 불문 프로토콜 지식. 스테이지 라우팅 판정의 재료.
//! 원칙: 방어 분류는 "차단 신호"에만 붙는다. CDN 존재 신호(cf-ray 등)는 2xx에선 정보일 뿐.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    None,
    Cloudflare,
    BotManager,
    RateLimit,
    Captcha,
    Unknown,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::None => "none",
            Class::Cloudflare => "cloudflare",
            Class::BotManager => "bot-manager",
            Class::RateLimit => "rate-limit",
            Class::Captcha => "captcha",
            Class::Unknown => "unknown",
        }
    }

    /// 분류별 다음 행동 힌트 — 스테이지 라우팅과 정렬.
    pub fn remedy(self) -> &'static str {
        match self {
            Class::Cloudflare => {
                "Pause requests to this website. Open it in your browser and check access."
            }
            Class::BotManager => {
                "Pause requests to this website. Check your account and the website's access requirements."
            }
            Class::RateLimit => {
                "Wait for Retry-After or the reset time before a bounded read retry."
            }
            Class::Captcha => {
                "Open the website and complete its check yourself. Continue other available work."
            }
            Class::Unknown => {
                "Pause this request. Use existing documentation and observations to choose the next step."
            }
            Class::None => "",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Observation {
    pub status_code: u16,
    pub headers: Vec<(String, String)>, // 소문자 키
    pub body_snippet: String,
}

impl Observation {
    pub fn header(&self, k: &str) -> &str {
        self.headers
            .iter()
            .find(|(hk, _)| hk == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
}

pub fn classify(o: &Observation) -> (Class, &'static str) {
    if o.status_code == 429 || !o.header("retry-after").is_empty() {
        return (Class::RateLimit, "status-429/retry-after");
    }
    if o.status_code < 400 {
        if body_hit(
            o,
            &["hcaptcha.com", "recaptcha/api", "geetest", "arkoselabs"],
        ) {
            return (Class::Captcha, "captcha-vendor-body-2xx");
        }
        return (Class::None, "");
    }
    if !o.header("cf-mitigated").is_empty() {
        return (Class::Cloudflare, "cf-mitigated");
    }
    if body_hit(
        o,
        &[
            "just a moment",
            "cf-chl",
            "challenge-platform",
            "/turnstile/",
        ],
    ) {
        return (Class::Cloudflare, "challenge-body");
    }
    if matches!(o.status_code, 401 | 404 | 405 | 410 | 422) {
        return (Class::None, "");
    }
    if !o.header("cf-ray").is_empty() || o.header("server") == "cloudflare" {
        return (Class::Cloudflare, "cf-block-headers");
    }
    if o.header("server").contains("akamai")
        || !o.header("x-akamai-transformed").is_empty()
        || !o.header("akamai-ref").is_empty()
    {
        return (Class::BotManager, "akamai-headers");
    }
    if body_hit(
        o,
        &["hcaptcha.com", "recaptcha/api", "geetest", "arkoselabs"],
    ) {
        return (Class::Captcha, "captcha-vendor-body");
    }
    if o.status_code == 403 || o.status_code == 503 {
        return (Class::Unknown, "403/503-no-signal");
    }
    (Class::None, "")
}

fn body_hit(o: &Observation, subs: &[&str]) -> bool {
    let b = o.body_snippet.to_lowercase();
    subs.iter().any(|s| b.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_presence_on_200_is_not_blocking() {
        let (cls, _) = classify(&Observation {
            status_code: 200,
            headers: vec![
                ("server".into(), "cloudflare".into()),
                ("cf-ray".into(), "x".into()),
            ],
            body_snippet: String::new(),
        });
        assert_eq!(cls, Class::None);
        for status in [401, 404, 405, 410, 422] {
            assert_eq!(
                classify(&Observation {
                    status_code: status,
                    headers: vec![
                        ("server".into(), "cloudflare".into()),
                        ("cf-ray".into(), "synthetic".into())
                    ],
                    body_snippet: "{\"error\":\"not available\"}".into()
                })
                .0,
                Class::None
            );
        }
    }

    #[test]
    fn challenge_body_is_cloudflare() {
        let (cls, _) = classify(&Observation {
            status_code: 403,
            headers: vec![("server".into(), "cloudflare".into())],
            body_snippet: "Just a moment...".into(),
        });
        assert_eq!(cls, Class::Cloudflare);
    }

    #[test]
    fn rate_limit() {
        let (cls, _) = classify(&Observation {
            status_code: 429,
            headers: vec![("retry-after".into(), "3".into())],
            body_snippet: String::new(),
        });
        assert_eq!(cls, Class::RateLimit);
    }
}
