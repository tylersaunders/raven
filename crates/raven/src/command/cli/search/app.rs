use std::error;

use super::duration::format_duration;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{
        HighlightSpacing, List, ListDirection, ListItem, ListState, Paragraph, StatefulWidgetRef,
        WidgetRef,
    },
};
use raven_database::HistoryFilters;
use raven_database::{Context, current_context, history::model::History};
use time::OffsetDateTime;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

/// The history scope of the current interactive session.
#[derive(Clone)]
pub enum Scope {
    Cwd,
    All,
}

pub struct SearchApp {
    pub running: bool,
    pub selected: Option<History>,
    input: String,
    cursor_position: usize,
    commands: Vec<History>,
    context: Context,
    now: Box<dyn Fn() -> OffsetDateTime + Send>,
}

#[derive(Clone)]
pub struct AppState {
    pub cusor_position: Position,
    pub list_state: ListState,
    pub scope: Scope,
    pub cwd: String,
    pub confirming_delete: bool,
}

impl SearchApp {
    /// Fetch a `History` list from the raven database which matches the current input query.
    pub fn get_history(&mut self, state: &AppState) {
        let results = match self.context.db.search(
            &self.input,
            HistoryFilters {
                exit: None,
                cwd: match state.scope {
                    Scope::Cwd => Some(self.context.cwd.clone()),
                    Scope::All => None,
                },
                limit: Some(500),
                suggest: false,
            },
        ) {
            Ok(h) => h,
            Err(err) => panic! {"{err}"},
        };
        self.commands = results;
    }

    pub fn get_history_count(&self) -> i64 {
        self.context.db.get_history_total().unwrap_or(-1)
    }

    pub fn new(query: String) -> Self {
        let pos = query.chars().count();
        Self {
            context: current_context(),
            running: true,
            input: query,
            cursor_position: pos,
            commands: Vec::new(),
            selected: None,
            now: Box::new(OffsetDateTime::now_utc),
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_right);
    }

    /// Prevent cursor from moving outside the input string.
    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor_position)
            .unwrap_or(self.input.len())
    }

    pub fn enter_char(&mut self, new_char: char, app_state: &AppState) {
        let idx = self.byte_index();
        self.input.insert(idx, new_char);
        self.move_cursor_right();
        self.get_history(app_state);
    }

    pub fn delete_char(&mut self, app_state: &AppState) {
        let is_not_cursor_leftmost = self.cursor_position != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_pos = self.cursor_position;
            let from_left_to_current_pos = current_pos - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_pos);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_pos);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
            self.get_history(app_state);
        }
    }

    /// Mark the list item at `idx` as selected and quit the search app.
    pub fn select(&mut self, idx: usize) {
        self.selected = Some(self.commands[idx].clone());
        self.quit();
    }

    /// Sets the app state to wait for delete confirmation.
    pub fn initiate_delete(state: &mut AppState) {
        if state.list_state.selected().is_some() {
            state.confirming_delete = true;
        }
    }

    /// Cancels the delete confirmation state.
    pub fn cancel_delete(state: &mut AppState) {
        state.confirming_delete = false;
    }

    /// Confirms the deletion of the selected item.
    /// TODO: Implement actual database deletion and error handling.
    pub fn confirm_delete(&mut self, state: &mut AppState) {
        if let Some(selected_index) = state.list_state.selected() {
            if selected_index < self.commands.len() {
                let item_to_delete = &self.commands[selected_index];
                let item_id = item_to_delete.id;

                // --- Placeholder for actual DB call ---
                // TODO: Add proper error handling and display to user in TUI
                match self.context.db.delete(item_id) {
                    Ok(()) => {
                        // Remove from the UI list *only on successful DB delete*
                        self.commands.remove(selected_index);

                        // Adjust selection after removal
                        if self.commands.is_empty() {
                            state.list_state.select(None);
                        } else if selected_index >= self.commands.len() {
                            // If the last item was deleted, select the new last item
                            state
                                .list_state
                                .select(Some(self.commands.len().saturating_sub(1)));
                        } else {
                            // Otherwise, the selection naturally moves to the next item,
                            // or stays if it was already pointing correctly.
                            // Ensure the index is valid if list shrunk
                            state.list_state.select(Some(
                                selected_index.min(self.commands.len().saturating_sub(1)),
                            ));
                        }
                    }
                    Err(e) => {
                        // TODO: Display this error in the TUI status bar instead of printing
                        eprintln!("Failed to delete history entry: {e}");
                    }
                }
                // --- End Placeholder ---
            }
        }
        // Always reset confirmation state after attempting
        state.confirming_delete = false;
    }
}

impl StatefulWidgetRef for &mut SearchApp {
    type State = AppState;

    fn render_ref(
        &self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) where
        Self: Sized,
    {
        // Layout locations
        let [header, hist_list, query_box, shortcuts] = Layout::vertical([
            Constraint::Length(4), // header
            Constraint::Min(5),    // hist_list
            Constraint::Length(5), // query_box
            Constraint::Length(5), // shortcuts
        ])
        .vertical_margin(4)
        .horizontal_margin(4)
        .areas(area);

        SearchApp::render_title(header, buf, self.get_history_count());
        SearchApp::render_history_list(
            hist_list,
            buf,
            &self.commands,
            &mut state.list_state,
            &self.now,
        );
        SearchApp::render_query_box(query_box, buf, self.input.as_str(), state);
        state.cusor_position = Position::new(
            query_box.x + 9 + u16::try_from(self.cursor_position).unwrap(),
            query_box.y + 1,
        );

        SearchApp::render_shortcuts(shortcuts, buf, state);
    }
}

impl SearchApp {
    /// Render the interactive screen header.
    fn render_title(area: Rect, buf: &mut Buffer, history_count: i64) {
        let [left, right] = Layout::horizontal([Constraint::Fill(1); 2]).areas(area);

        Paragraph::new(format!(
            "raven {VERSION}\n\
            Press Esc to exit.\n\
            History"
        ))
        .render_ref(left, buf);
        Paragraph::new(format!("history count: {history_count}"))
            .alignment(Alignment::Right)
            .render_ref(right, buf);
    }

    /// Renders the list of shell `History`
    ///
    /// * `history`: List of shell `History` objects to display
    /// * `list_state`: State object for the current list.
    /// * `now`: A fn that returns the current timestamp.
    fn render_history_list(
        area: Rect,
        buf: &mut Buffer,
        history: &[History],
        list_state: &mut ListState,
        now: &dyn Fn() -> OffsetDateTime,
    ) {
        let shortcuts = if list_state.selected().is_some() {
            let selected_idx = list_state.selected().unwrap();
            [
                selected_idx + 1,
                selected_idx + 2,
                selected_idx + 3,
                selected_idx + 4,
                selected_idx + 5,
            ]
        } else {
            [1, 2, 3, 4, 5]
        };
        StatefulWidgetRef::render_ref(
            &List::new(history.iter().enumerate().map(|(i, h)| {
                let shortcut = if shortcuts.contains(&i) {
                    Some(shortcuts.iter().position(|&pos| pos == i).expect("not in") + 1)
                } else {
                    None
                };
                SearchApp::history_to_list_item(h, now, shortcut)
            }))
            .highlight_style(Style::default().fg(Color::Green))
            .highlight_symbol(">>")
            .highlight_spacing(HighlightSpacing::Always)
            .direction(ListDirection::BottomToTop),
            area,
            buf,
            list_state,
        );
    }

    /// Renders the cursor and input query.
    fn render_query_box(area: Rect, buf: &mut Buffer, input: &str, app_state: &AppState) {
        let [top, bottom] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
            .horizontal_margin(9)
            .vertical_margin(1)
            .areas(area);

        let scope = match app_state.scope {
            Scope::Cwd => app_state.cwd.as_str(),
            Scope::All => "(Everything)",
        };

        Paragraph::new(scope)
            .style(Style::new().light_cyan())
            .render_ref(bottom, buf);
        Paragraph::new(input)
            .style(Style::default().yellow())
            .render_ref(top, buf);
    }

    /// Renders the shortcuts or a confirmation prompt in the specified area.
    ///
    /// Depending on the `confirming_delete` state in `AppState`, this function
    /// either displays the standard shortcuts (Tab, Alt+1..5, Alt+d) or a
    /// confirmation prompt for deleting an entry.
    ///
    /// # Arguments
    ///
    /// * `area` - The `Rect` area where the shortcuts or prompt should be rendered.
    /// * `buf` - The `Buffer` to render onto.
    /// * `state` - The current `AppState` containing the application state.
    fn render_shortcuts(area: Rect, buf: &mut Buffer, state: &AppState) {
        let [top, bottom] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        if state.confirming_delete {
            // Render confirmation prompt
            let confirm_text = Line::from(vec![
                Span::styled("Delete entry? ", Style::default().fg(Color::Yellow)),
                Span::styled("[y]", Style::default().fg(Color::Green).bold()),
                Span::styled("/", Style::default().fg(Color::Gray)),
                Span::styled("[N]", Style::default().fg(Color::Red).bold()),
            ]);
            Paragraph::new(confirm_text)
                .alignment(Alignment::Left)
                .render_ref(bottom, buf); // Render prompt in the bottom area
            // Optionally clear the top area or change the title if needed
            Paragraph::new("Confirm Delete").render_ref(top, buf); // Change title
        } else {
            // Render normal shortcuts
            Paragraph::new("Shortcuts").render_ref(top, buf); // Keep original title
            let tab = Line::default()
                .spans([Span::default().content("<TAB>: Toggle cwd or Global scope")]);
            let quick_pick = Line::default().spans([
                Span::default().content("<Alt + "),
                Span::default().fg(Color::Magenta).content("1..5"),
                Span::default().content(">: Quick Pick"),
            ]);
            let delete_key = Line::default()
                .spans([Span::default().content("<Alt + d>: Delete selected entry")]);
            let shortcuts = List::new([tab, quick_pick, delete_key]);
            WidgetRef::render_ref(&shortcuts, bottom, buf);
        }
    }

    /// Generates a `ListItem` for the provided `History`.
    fn history_to_list_item<'a>(
        h: &'a History,
        now: &dyn Fn() -> OffsetDateTime,
        shortcut: Option<usize>,
    ) -> ListItem<'a> {
        let shortcut_span = if shortcut.is_some() {
            Span::styled(format!(" {}", shortcut.unwrap()), Style::new().magenta())
        } else {
            Span::default().content("  ")
        };

        let line = Line::default().spans([
            // Shortcut
            shortcut_span,
            // The time since the command was run, color coded by exit_code
            Span::styled(
                format!("{:>4}", SearchApp::time_since(&now, h)),
                match h.exit_code {
                    0 => Style::new().blue(),
                    _ => Style::new().red(),
                },
            ),
            // The command itself
            Span::styled(format!(" {}", h.command), Style::default()),
        ]);
        ListItem::new(line)
    }

    /// Get a duration string for how long it has been since the command was run.
    ///
    /// * `now`: Function which returns the current time
    /// * `then`: The command
    fn time_since(now: &dyn Fn() -> OffsetDateTime, then: &History) -> String {
        let since = (now()) - then.timestamp;
        format_duration(since.try_into().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend, layout::Position};

    use raven_common::config::config::Config;
    use raven_database::database::{Database, DatabaseError};
    use time::{Duration, OffsetDateTime};

    // Helper to create a default AppState for tests
    fn default_app_state() -> AppState {
        AppState {
            cusor_position: Position::default(),
            list_state: ListState::default(),
            scope: Scope::All,
            cwd: String::from("/test/dir"),
            confirming_delete: false, // Initialize here
        }
    }
    // --- Mock Database for Testing ---

    #[derive(Clone, Default)]
    struct MockDb {
        // Store history items added for testing search results
        mock_history: Vec<History>,
        // Control the total history count returned
        mock_total_count: i64,
    }

    // Implement the methods SearchApp actually calls on the database connection.
    // Adjust this based on the actual trait/methods used by `context.db`.
    // Assuming `context.db` is something like `Arc<dyn DatabaseConnection>`
    // or a concrete type with these methods. If it's a trait, implement the trait.
    // For simplicity, implementing inherent methods here.
    impl MockDb {
        fn search(&self, query: &str, _filters: HistoryFilters) -> Vec<History> {
            let results = self
                .mock_history
                .iter()
                .filter(|h| h.command.contains(query))
                .cloned()
                .collect();
            results
        }

        fn get_history_total(&self) -> i64 {
            self.mock_total_count
        }

        // Helper to set up mock data for a test
        #[allow(dead_code)] // Used by tests implicitly via create_test_app
        fn set_history(&mut self, history: Vec<History>) {
            self.mock_history = history;
        }

        // Helper to set mock total count
        #[allow(dead_code)] // Used by tests implicitly via create_test_app
        fn set_total_count(&mut self, count: i64) {
            self.mock_total_count = count;
        }
    }

    // Assuming DatabaseConnection is a trait your actual DB connection implements
    // If not, adjust the MockDb methods above and how it's used in Context.
    // This impl might be needed if `context.db` is `Arc<dyn DatabaseConnection>`
    impl Database for MockDb {
        fn search(
            &self,
            query: &str,
            filters: HistoryFilters,
        ) -> Result<Vec<History>, DatabaseError> {
            // Delegate to inherent method
            Ok(self.search(query, filters))
        }
        fn get_history_total(&self) -> Result<i64, raven_database::database::DatabaseError> {
            // Delegate to inherent method
            Ok(self.get_history_total())
        }

        fn save(
            &mut self,
            _history: &History,
        ) -> Result<i64, raven_database::database::DatabaseError> {
            unimplemented!()
        }

        fn save_bulk(
            &mut self,
            _history: &[History],
        ) -> Result<Vec<i64>, raven_database::database::DatabaseError> {
            unimplemented!()
        }

        fn get(
            &self,
            _id: i64,
        ) -> Result<Option<History>, raven_database::database::DatabaseError> {
            unimplemented!()
        }

        fn update(
            &self,
            _history: &History,
        ) -> Result<(), raven_database::database::DatabaseError> {
            unimplemented!()
        }

        fn delete(&self, _id: i64) -> Result<(), DatabaseError> {
            unimplemented!()
        }
        // ... etc for other trait methods
    }

    // --- End Mock Database ---

    // Helper function to create a SearchApp instance for testing.
    // Note: This uses current_context(), which might have side effects or
    // require specific environment setup if it tries to access a real database.
    // For more robust tests, consider mocking the Context/DatabaseConnection.

    fn create_test_app(initial_input: &str) -> SearchApp {
        // Create the mock database and context
        let mut mock_db = Box::new(MockDb::default());
        let fake_history = Vec::from([
            History {
                id: 1,
                command: "cmd1".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 2,
                command: "cmd2".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 3,
                command: "cmd3".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 4,
                command: "cmd4".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 5,
                command: "cmd5".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 6,
                command: "cmd6".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 7,
                command: "cmd7".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
        ]);
        mock_db.set_history(fake_history);
        let mock_context = Context {
            // Set a fixed CWD for tests
            cwd: "/test/dir".to_string(),
            // Use the mock database instance
            // Adjust this line based on how `db` is stored in your `Context` struct
            // e.g., if it's Box<dyn T>, Arc<dyn T>, or a concrete type.
            db: mock_db, // Assuming Arc<dyn Trait>
            // Add other Context fields if necessary, using default/test values
            config: Config::default(), // Or a specific test config
        };

        let pos = initial_input.chars().count();
        SearchApp {
            context: mock_context, // Use the mocked context
            running: true,
            input: initial_input.to_string(),
            cursor_position: pos,
            commands: Vec::new(),
            selected: None,
            // Fixed 'now' function for predictable time tests
            now: Box::new(|| OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()),
        }
    }

    #[test]
    fn test_clamp_cursor() {
        let app = create_test_app("hello"); // len 5, indices 0..4
        assert_eq!(app.clamp_cursor(0), 0);
        assert_eq!(app.clamp_cursor(3), 3);
        assert_eq!(app.clamp_cursor(5), 5); // Clamped to length
        assert_eq!(app.clamp_cursor(10), 5); // Clamped to length
    }

    #[test]
    fn test_move_cursor_left() {
        let mut app = create_test_app("test");
        app.cursor_position = 3;
        app.move_cursor_left();
        assert_eq!(app.cursor_position, 2);
        app.move_cursor_left();
        app.move_cursor_left();
        assert_eq!(app.cursor_position, 0);
        app.move_cursor_left(); // Try moving past beginning
        assert_eq!(app.cursor_position, 0);
    }

    #[test]
    fn test_move_cursor_right() {
        let mut app = create_test_app("test"); // len 4
        app.cursor_position = 1;
        app.move_cursor_right();
        assert_eq!(app.cursor_position, 2);
        app.move_cursor_right();
        app.move_cursor_right();
        assert_eq!(app.cursor_position, 4);
        app.move_cursor_right(); // Try moving past end
        assert_eq!(app.cursor_position, 4);
    }

    #[test]
    fn test_byte_index() {
        let mut app = create_test_app("a");
        app.cursor_position = 0;
        assert_eq!(app.byte_index(), 0);
        app.cursor_position = 1;
        assert_eq!(app.byte_index(), 1); // End of string

        let mut app_multi = create_test_app("你好"); // "ni hao" - 2 chars, 6 bytes
        app_multi.cursor_position = 0;
        assert_eq!(app_multi.byte_index(), 0);
        app_multi.cursor_position = 1; // After '你'
        assert_eq!(app_multi.byte_index(), 3);
        app_multi.cursor_position = 2; // After '好' (end of string)
        assert_eq!(app_multi.byte_index(), 6);
    }

    #[test]
    fn test_enter_char() {
        let mut app = create_test_app("test");
        let state = default_app_state();
        app.cursor_position = 2; // te|st
        app.enter_char('X', &state); // Should become teX|st
        assert_eq!(app.input, "teXst");
        assert_eq!(app.cursor_position, 3);

        app.cursor_position = 0; // |teXst
        app.enter_char('Y', &state); // Should become Y|teXst
        assert_eq!(app.input, "YteXst");
        assert_eq!(app.cursor_position, 1);

        app.cursor_position = app.input.chars().count(); // YteXst|
        app.enter_char('Z', &state); // Should become YteXstZ|
        assert_eq!(app.input, "YteXstZ");
        assert_eq!(app.cursor_position, 7);
    }

    #[test]
    fn test_delete_char() {
        let mut app = create_test_app("test");
        let state = default_app_state();
        app.cursor_position = 3; // tes|t
        app.delete_char(&state); // Should become te|t
        assert_eq!(app.input, "tet");
        assert_eq!(app.cursor_position, 2);

        app.cursor_position = 1; // t|et
        app.delete_char(&state); // Should become |et
        assert_eq!(app.input, "et");
        assert_eq!(app.cursor_position, 0);

        app.delete_char(&state); // Cursor at 0, should do nothing
        assert_eq!(app.input, "et");
        assert_eq!(app.cursor_position, 0);

        let mut app_multi = create_test_app("你好"); // ni hao
        app_multi.cursor_position = 1; // 你|好
        app_multi.delete_char(&state); // Should become |好
        assert_eq!(app_multi.input, "好");
        assert_eq!(app_multi.cursor_position, 0);
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_time_since() {
        // Fixed "now" time: 1700000000 (2023-11-14 22:13:20 UTC)
        let now_fn = || OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();

        // History entry 5 seconds ago
        let hist_5s = History {
            timestamp: now_fn() - Duration::seconds(5),
            command: "cmd1".to_string(),
            exit_code: 0,
            cwd: String::new(),
            id: 1,
        };
        assert_eq!(SearchApp::time_since(&now_fn, &hist_5s), "5s");

        // History entry 2 minutes ago
        let hist_2m = History {
            timestamp: now_fn() - Duration::minutes(2),
            command: "cmd2".to_string(),
            exit_code: 0,
            cwd: String::new(),
            id: 2,
        };
        assert_eq!(SearchApp::time_since(&now_fn, &hist_2m), "2m");

        // History entry 3 hours ago
        let hist_3h = History {
            timestamp: now_fn() - Duration::hours(3),
            command: "cmd3".to_string(),
            exit_code: 0,
            cwd: String::new(),
            id: 3,
        };
        assert_eq!(SearchApp::time_since(&now_fn, &hist_3h), "3h");

        // History entry 4 days ago
        let hist_4d = History {
            timestamp: now_fn() - Duration::days(4),
            command: "cmd4".to_string(),
            exit_code: 0,
            cwd: String::new(),
            id: 4,
        };
        assert_eq!(SearchApp::time_since(&now_fn, &hist_4d), "4d");

        // History entry just now (or slightly in future due to precision)
        let hist_now = History {
            timestamp: now_fn(),
            command: "cmd5".to_string(),
            exit_code: 0,
            cwd: String::new(),
            id: 5,
        };
        assert_eq!(SearchApp::time_since(&now_fn, &hist_now), "0s"); // Assuming format_duration handles 0 correctly
    }

    #[test]
    fn test_quit() {
        let mut app = create_test_app("");
        assert!(app.running);
        app.quit();
        assert!(!app.running);
    }

    #[test]
    fn test_select() {
        let mut app = create_test_app("");
        app.commands = vec![
            History {
                id: 1,
                command: "cmd1".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
            History {
                id: 2,
                command: "cmd2".to_string(),
                timestamp: OffsetDateTime::now_utc(),
                exit_code: 0,
                cwd: String::new(),
            },
        ];
        assert!(app.selected.is_none());
        assert!(app.running);

        app.select(1); // Select the second command ("cmd2")

        assert!(app.selected.is_some());
        assert_eq!(app.selected.unwrap().command, "cmd2");
        assert!(!app.running); // Selecting should also quit
    }

    #[test]
    fn test_render_app() {
        let mut app = create_test_app("cmd");
        let mut app_state = AppState {
            cusor_position: Position::default(),
            list_state: ListState::default(),
            scope: Scope::Cwd,
            cwd: String::new(),
            confirming_delete: false, // Initialize here
        };
        app.get_history(&app_state);
        println!("{:?}", app.commands);
        app_state.list_state.select_first();
        let mut terminal = Terminal::new(TestBackend::new(80, 40)).unwrap();
        let _ = terminal
            .draw(|frame| frame.render_stateful_widget_ref(&mut app, frame.area(), &mut app_state));
        assert_snapshot!(terminal.backend());
    }
}
