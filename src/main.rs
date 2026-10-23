#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod core;
#[cfg(windows)]
mod hardware;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod runtime;
mod storage;
#[cfg(windows)]
mod ui;

fn main() {
    #[cfg(windows)]
    if let Err(error) = run() {
        platform::error(&error);
    }
}
#[cfg(windows)]
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--diagnose") => {
            let path = args
                .get(1)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| storage::data_dir().join("diagnostic.json"));
            storage::write_json(&path, &hardware::diagnostic())
        }
        Some("--guard") => runtime::guard(&args[1..]),
        Some("--show") => ui::run(true),
        Some("--exit") => ui::request_exit(),
        Some("--smoke-test") => {
            ui::smoke_test(args.get(1).ok_or("Informe o arquivo de resultado")?)
        }
        None => ui::run(false),
        _ => Err(
            "Argumento desconhecido. Use --diagnose, --show ou abra o aplicativo normalmente."
                .into(),
        ),
    }
}
