//! Synthetic Windows URL launcher for CLI integration tests. Never opens a browser.
fn main() {
    let capture = std::env::var_os("HYCLI_FEEDBACK_CAPTURE").expect("capture path");
    let arguments = std::env::args().skip(1).collect::<Vec<_>>().join("\n");
    std::fs::write(capture, arguments).expect("record launcher arguments");
    std::process::exit(3);
}
