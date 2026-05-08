fn main() {
    // Minimal binary wrapper — core logic lives in the library to keep main.rs clean for tests.
    let args: Vec<String> = std::env::args().collect();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(e) = rt.block_on(kazane_game_launcher::run_from_args(args)) {
        eprintln!("Error: {}", e);
    }
}
