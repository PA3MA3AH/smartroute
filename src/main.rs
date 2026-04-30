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
mod config_editor;

fn main() -> anyhow::Result<()> {
    cli::run()
}
