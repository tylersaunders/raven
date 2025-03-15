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
use raven_database::{Context, current_context, history::model::History};
use time::OffsetDateTime;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application result type.
pub type AppResult<T> = std::result::Result<T, Box<dyn error::Error>>;

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
}

impl SearchApp {
    /// Fetch a `History` list from the raven database which matches the current input query.
    pub fn get_history(&mut self) {
        let Ok(results) = self.context.db.search(&self.input, Some(500)) else {
            panic!("error")
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

    pub fn enter_char(&mut self, new_char: char) {
        let idx = self.byte_index();
        self.input.insert(idx, new_char);
        self.move_cursor_right();
        self.get_history();
    }

    pub fn delete_char(&mut self) {
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
            self.get_history();
        }
    }

    /// Mark the list item at `idx` as selected and quit the search app.
    pub fn select(&mut self, idx: usize) {
        self.selected = Some(self.commands[idx].clone());
        self.quit();
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
        let [top, middle, bottom] = Layout::vertical(Constraint::from_percentages([25, 60, 15]))
            .vertical_margin(4)
            .horizontal_margin(4)
            .areas(area);

        SearchApp::render_title(top, buf, self.get_history_count());
        SearchApp::render_history_list(
            middle,
            buf,
            &self.commands,
            &mut state.list_state,
            &self.now,
        );
        SearchApp::render_query_box(bottom, buf, self.input.as_str());
        state.cusor_position = Position::new(
            bottom.x + u16::try_from(self.cursor_position).unwrap(),
            bottom.y,
        );
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
        // TODO: get a count query into the UI.
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
        StatefulWidgetRef::render_ref(
            &List::new(
                history
                    .iter()
                    .map(|h| SearchApp::history_to_list_item(h, now)),
            )
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
    fn render_query_box(area: Rect, buf: &mut Buffer, input: &str) {
        Paragraph::new(input)
            .style(Style::default().yellow())
            .render_ref(area, buf);
    }

    /// Generates a `ListItem` for the provided `History`.
    fn history_to_list_item<'a>(h: &'a History, now: &dyn Fn() -> OffsetDateTime) -> ListItem<'a> {
        let line = Line::default().spans([
            // Padding
            Span::default().content(" "),
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
