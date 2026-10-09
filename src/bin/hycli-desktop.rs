//! Tiny native launcher; the CLI remains a console executable for terminals and MCP.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::Command;

fn launch() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::current_exe()?
        .parent()
        .ok_or("Missing application directory")?
        .to_owned();
    let mut command =
        Command::new(directory.join(if cfg!(windows) { "hycli.exe" } else { "hycli" }));
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        command.arg("open");
    } else {
        command.args(args);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        // Finder/application menus do not read interactive shell PATH configuration.
        let mut paths: Vec<_> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
        if let Some(home) = std::env::var_os("HOME") {
            for suffix in [".local/bin", ".cargo/bin"] {
                paths.push(std::path::Path::new(&home).join(suffix));
            }
        }
        for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            paths.push(directory.into());
        }
        command.env("PATH", std::env::join_paths(paths)?);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "Hycli could not complete this action.\n{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )
        .into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = launch() {
        let message = error.to_string();
        eprintln!("{message}");
        #[cfg(windows)]
        {
            #[link(name = "user32")]
            unsafe extern "system" {
                fn MessageBoxW(
                    window: *mut std::ffi::c_void,
                    text: *const u16,
                    title: *const u16,
                    flags: u32,
                ) -> i32;
            }
            let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
            let title: Vec<_> = "Hycli".encode_utf16().chain(Some(0)).collect();
            // Both UTF-16 strings remain valid and NUL-terminated for this synchronous call.
            unsafe {
                MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
            }
        }
        #[cfg(target_os = "macos")]
        let _ = Command::new("/usr/bin/osascript")
            .args([
                "-e",
                "on run argv\ndisplay alert \"Hycli\" message (item 1 of argv)\nend run",
                &message,
            ])
            .status();
        #[cfg(target_os = "linux")]
        if Command::new("zenity")
            .args(["--error", "--title=Hycli", "--text", &message])
            .status()
            .is_err()
        {
            if Command::new("kdialog")
                .args(["--error", &message, "--title", "Hycli"])
                .status()
                .is_err()
            {
                let _ = Command::new("notify-send")
                    .args(["Hycli", &message])
                    .status();
            }
        }
        std::process::exit(1);
    }
}
