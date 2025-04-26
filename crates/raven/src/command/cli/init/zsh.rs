const BIND_UP_ARROW: &str = r"bindkey -M emacs '^[[A' raven-search-history-up
bindkey -M vicmd '^[[A' raven-search-history-up
bindkey -M viins '^[[A' raven-search-history-up
bindkey -M emacs '^[OA' raven-search-history-up
bindkey -M vicmd '^[OA' raven-search-history-up
bindkey -M viins '^[OA' raven-search-history-up
bindkey -M vicmd 'k' raven-search-history-up";

const BIND_CTRL_R: &str = r"bindkey -M emacs '^r' raven-search-history
bindkey -M viins '^r' raven-search-history
bindkey -M vicmd '/' raven-search-history";

/// Initialize Raven for the zsh shell.
pub fn init() {
    init_static();

    println!("{BIND_UP_ARROW}");
    println!("{BIND_CTRL_R}");
}

/// Emits the static zsh initialization script to stdout
fn init_static() {
    let base = include_str!("../../../shell/raven.zsh");
    println!("{base}");
}
