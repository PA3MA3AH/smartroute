mod cli;
mod config;
mod config_editor;
mod daemon;
mod diagnosis;
mod picker;
mod runtime;
mod singbox;
mod subscription;
mod tester;
mod util;

fn main() -> anyhow::Result<()> {
    cli::run()
}
