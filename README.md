# Raven: Your Shell's Wise Old Bird for History 🐦‍⬛

[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/tylersaunders/raven/ci.yml)](https://github.com/tylersaunders/raven/actions/workflows/ci.yml)
[![GitHub License](https://img.shields.io/github/license/tylersaunders/raven)](https://github.com/tylersaunders/raven/blob/main/LICENSE)
[![GitHub Release](https://img.shields.io/github/v/release/tylersaunders/raven)](https://github.com/tylersaunders/raven/releases/latest)
[![Bluesky](https://img.shields.io/badge/Bluesky-0285FF?logo=bluesky&logoColor=fff)](https://bsky.app/profile/tyler.thesummit.dev)

Tired of `Ctrl+R` feeling like rummaging through a messy drawer? Do `.histfile` or `.zsh_history` files grow larger than your patience? Let **Raven** perch in your terminal and bring order to your command history chaos!

Raven cleverly swaps out your traditional, clunky history file for a sleek, searchable database. It doesn't just *store* your commands; it gives you a nifty interactive UI right in your shell to find exactly what you need, when you need it. Nevermore shall you lose that crucial command!

## Caw-se for Celebration! (Why Raven?) ✨

*   **Say Goodbye to Endless Scrolling:** Forget grepping giant history files. Raven's database is *fast*.
*   **Interactive Search:** A slick Terminal UI (TUI) lets you search, filter, and browse your history like a pro. It's like having a command concierge!
*   **Context is King:** Imagine storing *when* you ran a command, *where* (directory), and even its exit status! Raven aims to remember the important details.
*   **Never Forget:** Your history is safe in a database, less prone to accidental truncation or corruption.

## Features That'll Make You Flock to Raven 🪶

*   **Database Backend:** Stores shell history efficiently (SQLite by default).
*   **Interactive TUI:** A beautiful and intuitive interface for searching history.
*   **Seamless Integration:** Hooks into your shell to automatically capture commands.
*   **Existing History:** Your standard shell history file (`.zsh_history`, `.histfile`, etc.) is untouched and can be imported into Raven.
*   **Shell Support:** Currently, only Zsh is officially supported. (Bash support planned!)
*   **Private & Local:** All your history data stays securely on your device. No cloud sync, no telemetry, no third-party servers involved.
*   **Smart Searching:** Text search and Filter by the current directory, exit codes etc.
*   **Configuration:** Customize Raven's behavior to your liking.

## Installation: Get Your Raven Perched 🛠️

*(Instructions will vary depending on the actual implementation)*

**Option 1: Grab the pre-built binary (The Easiest Nest)**

TODO: Setup Github actions for release builds.

**Option 2: Build from Source (For the Brave Crows)**

```sh
git clone https://github.com/tylersaunders/raven.git
cd raven
# Follow build instructions (e.g., make, go build, cargo build)
cargo build --bin raven --release

# Release binary will be raven/target/release/raven
```

**Post-Installation:**

Ensure the `raven` binary is on your shell's path.
You'll need to add a hook to your shell's configuration file (`.zshrc`):

```sh
# Example for .zshrc
eval "$(raven init zsh)"
```

Follow the specific instructions provided by `raven init --help` or the installation process.

## Usage: Summoning Your Commands 🪄

1.  **Automatic Capture:** Just use your shell as you normally would. Raven silently observes and records your commands in the background.
2.  **Interactive Search:** Press a configured keybinding (e.g., `Ctrl+R` - configurable, of course!) to launch the Raven TUI.
3.  **Search & Select:** Type to search, use arrow keys to navigate, and press `Enter` to select a command and place it on your prompt. `Esc` to quit.

*(Add more specific usage details, keybindings, or examples as needed)*

## Contributing: Join the Flock! 🤝

Found a bug? Have a neat idea? Contributions are welcome! Please check the `CONTRIBUTING.md` file for guidelines. Let's make Raven the wisest bird in the terminal!

## License 📜

This project is licensed under the GPL-3.0 - see the `LICENSE` file for details.

---

Happy commanding! May your history always be easy to find. Quoth the Raven, "Search some more!"
