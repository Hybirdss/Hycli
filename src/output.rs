//! I/O 계약: stdout = 데이터 전용(non-TTY 자동 JSON), stderr = 로그·에러.
use std::io::{self, IsTerminal, Write};

use crate::apperr::{self, AppError};

pub struct Emitter {
    json: bool,
}

impl Emitter {
    pub fn new(force_json: bool) -> Self {
        let tty = io::stdout().is_terminal();
        Self {
            json: force_json || !tty,
        }
    }

    pub fn data(&self, v: &impl serde::Serialize) {
        let mut out = io::stdout();
        let value = serde_json::to_value(v).unwrap_or(serde_json::Value::Null);
        let pretty = if self.json {
            serde_json::to_string(&value)
        } else {
            serde_json::to_string_pretty(&value)
        }
        .unwrap_or_default();
        let _ = writeln!(out, "{pretty}");
    }

    /// 표준 오류 한 줄 + exit code 반환.
    pub fn fail(&self, err: &AppError) -> i32 {
        let mut errw = io::stderr();
        let _ = writeln!(errw, "hycli: code={} {err}", apperr::code_of(err).as_i32());
        apperr::code_of(err).as_i32()
    }
}
