//! I/O contract: stdout is reserved for structured data (compact JSON when non-TTY); stderr is for diagnostics and errors.
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

    pub fn data(&self, v: &impl serde::Serialize) -> Result<(), AppError> {
        let mut out = io::stdout();
        let value = serde_json::to_value(v).unwrap_or(serde_json::Value::Null);
        let pretty = if self.json {
            serde_json::to_string(&value)
        } else {
            serde_json::to_string_pretty(&value)
        }
        .unwrap_or_default();
        writeln!(out, "{pretty}")
            .and_then(|()| out.flush())
            .map_err(|error| {
                apperr::runtime(
                    error,
                    "Cannot write stdout",
                    "Check the output destination.",
                )
            })
    }

    /// Emit a single-line error to stderr and return the exit code.
    pub fn fail(&self, err: &AppError) -> i32 {
        let mut errw = io::stderr();
        let _ = writeln!(errw, "hycli: code={} {err}", apperr::code_of(err).as_i32());
        if err.diagnostic.is_some() {
            let _ = writeln!(errw, "{}", err.public_error());
        }
        apperr::code_of(err).as_i32()
    }
}
