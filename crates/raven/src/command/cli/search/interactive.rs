use std::io::{self};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Position;
use ratatui::widgets::ListState;
use ratatui::{Terminal, prelude::CrosstermBackend};
use raven_database::history::model::History;

use super::app::{SearchApp, AppState};
use super::event::{Event, EventHandler};
use super::tui::Tui;

#[allow(clippy::unnecessary_wraps)]
pub fn history(query: &[String]) -> Option<History> {

    let mut app = SearchApp::new(query.join(" "));
    // Fetch initial list
    app.get_history();

    let backend = CrosstermBackend::new(io::stderr());
    let terminal = Terminal::new(backend).unwrap();
    let events = EventHandler::new(250);
    let mut tui = Tui::new(terminal, events);
    tui.init().unwrap();

    // Establish initial cursor state, this will get updated each draw.
    let mut app_state = AppState {
        cusor_position: Position::default(),
        list_state: ListState::default(),
    };

    app_state.list_state.select_first();

    while app.running {
        tui.draw(&mut app, &mut app_state).unwrap();
        match tui.events.next().unwrap() {
            Event::Key(key_event) => handle_key_events(key_event, &mut app, &mut app_state),
            Event::Mouse(_) | Event::Resize(_, _) | Event::Tick => {},
        }
    }
    tui.exit().unwrap();


    app.selected
}

/// Handles the key events and updates the state of [`App`].
pub fn handle_key_events(key_event: KeyEvent, app: &mut SearchApp, state: &mut AppState) {
    match key_event.code {
        // Exit application on `ESC` or `q`
        KeyCode::Esc => {
            app.quit();
        }
        KeyCode::Left => app.move_cursor_left(),
        KeyCode::Right => app.move_cursor_right(),
        KeyCode::Char(to_insert) => app.enter_char(to_insert),
        KeyCode::Backspace => app.delete_char(),
        KeyCode::Enter => {
            if let Some(idx) = state.list_state.selected() {
                app.select(idx);
            }
        }
        KeyCode::Up => state.list_state.select_next(),
        KeyCode::Down => state.list_state.select_previous(),
        _ => {}
    }
}
