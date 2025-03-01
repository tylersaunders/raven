
/// Initialize Raven for the zsh shell.
pub fn init() {
    init_static();
}

/// Emits the static zsh initialization script to stdout
fn init_static() {
    let base = include_str!("../../../shell/raven.zsh");
    println!("{base}");
}
