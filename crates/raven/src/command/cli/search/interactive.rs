use std::io::{self};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Position;
use ratatui::widgets::ListState;
use ratatui::{Terminal, prelude::CrosstermBackend};
use raven_common::utils;
use raven_database::history::model::History;

use super::app::{AppState, Scope, SearchApp};
use super::event::{Event, EventHandler};
use super::tui::Tui;

#[allow(clippy::unnecessary_wraps)]
pub fn history(query: &[String]) -> Option<History> {
    let mut app = SearchApp::new(query.join(" "));

    // Establish initial cursor state, this will get updated each draw.
    let mut app_state = AppState {
        cusor_position: Position::default(),
        list_state: ListState::default(),
        scope: Scope::Cwd,
        cwd: utils::get_current_dir(),
    };

    // Fetch initial list
    app.get_history(&app_state);

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend).unwrap();
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);
    tui.init().unwrap();

    app_state.list_state.select_first();

    while app.running {
        tui.draw(&mut app, &mut app_state).unwrap();
        match tui.events.next().unwrap() {
            Event::Key(key_event) => handle_key_events(key_event, &mut app, &mut app_state),
            Event::Mouse(_) | Event::Resize(_, _) | Event::Tick => {}
        }
    }
    tui.exit().unwrap();

    app.selected
}

/// Handles the key events and updates the state of [`App`].
pub fn handle_key_events(key_event: KeyEvent, app: &mut SearchApp, state: &mut AppState) {
    match (key_event.modifiers, key_event.code) {
        // Exit application on `ESC` or `q`
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.quit();
        }
        (KeyModifiers::NONE, KeyCode::Left) => app.move_cursor_left(),
        (KeyModifiers::NONE, KeyCode::Right) => app.move_cursor_right(),
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(to_insert)) => {
            app.enter_char(to_insert, state);
        }
        (KeyModifiers::ALT, KeyCode::Char(shortcut)) => {
            let shortcuts = ['1', '2', '3', '4', '5'];

            if shortcuts.contains(&shortcut) {
                if let Some(offset) = shortcut.to_digit(10) {
                    let current = state.list_state.selected().unwrap_or(0);
                    let pos = current + offset as usize;
                    app.select(pos);
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => app.delete_char(state),
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if let Some(idx) = state.list_state.selected() {
                app.select(idx);
            }
        }
        (KeyModifiers::NONE, KeyCode::Up) => state.list_state.select_next(),
        (KeyModifiers::NONE, KeyCode::Down) => state.list_state.select_previous(),
        (KeyModifiers::NONE, KeyCode::Tab) => {
            match state.scope {
                super::app::Scope::Cwd => state.scope = Scope::All,
                super::app::Scope::All => state.scope = Scope::Cwd,
            }
            app.get_history(state);
        }
        _ => {}
    }
}
