mod autostart;
mod backup;
mod cli;
mod config;
mod config_editor;
mod daemon;
mod diagnosis;
mod dnstest;
mod doctor;
mod health;
mod killswitch;
mod leaktest;
mod mask;
mod picker;
mod resolve;
mod runtime;
mod singbox;
mod subscription;
mod tester;
mod util;
mod whitelist;

fn main() -> anyhow::Result<()> {
    cli::run()
}
