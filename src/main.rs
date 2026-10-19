use clap::Parser;
use hycli::cli::{Cli, Cmd, Deps, run};
use hycli::output::Emitter;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let deps = Deps {
        kb: cli.kb,
        site: cli.site.unwrap_or_else(|| "default".into()),
        force_json: cli.format == "json",
        allow_local: cli.allow_local,
    };
    let emit = Emitter::new(deps.force_json);
    let code = run(
        deps,
        cli.cmd.unwrap_or(Cmd::Open {
            port: 4318,
            no_open: false,
        }),
        &emit,
    )
    .await;
    std::process::exit(code);
}
