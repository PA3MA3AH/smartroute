mod cli;
mod config;
mod diagnosis;
mod picker;
mod runtime;
mod singbox;
mod subscription;
mod tester;
mod util;
mod daemon;

fn main() -> anyhow::Result<()> {
    cli::run()
}
