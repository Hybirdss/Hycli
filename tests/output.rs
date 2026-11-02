use std::process::Command;
#[cfg(unix)]
use std::process::{Output, Stdio};

fn version_command() -> Command {
    let root = std::env::temp_dir().join(format!("hycli-output-{}", hycli::util::id()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_hycli"));
    command
        .arg("version")
        .env("HYCLI_DATA_DIR", &root)
        .env("HYCLI_TEST_DATA", &root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"));
    command
}

#[cfg(unix)]
fn assert_output_failure(output: Output) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains("code=1"), "{diagnostic}");
    assert!(diagnostic.contains("stdout"), "{diagnostic}");
}

#[test]
fn version_emits_compact_json_with_a_newline() {
    let output = version_command().output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        format!("{{\"version\":\"{}\"}}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn version_reports_a_full_stdout_sink() {
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let output = version_command()
        .stdout(Stdio::from(sink))
        .output()
        .unwrap();
    assert_output_failure(output);
}

#[cfg(unix)]
#[test]
fn version_reports_a_closed_stdout_reader() {
    let (reader, writer) = std::os::unix::net::UnixStream::pair().unwrap();
    drop(reader);
    let writer: std::os::fd::OwnedFd = writer.into();
    let output = version_command()
        .stdout(Stdio::from(writer))
        .output()
        .unwrap();
    assert_output_failure(output);
}
