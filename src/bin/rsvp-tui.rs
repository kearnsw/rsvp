//! RSVP Reader - Terminal-based Rapid Serial Visual Presentation speed reader
//!
//! Hotkeys:
//!   Space       - Start/Pause reading
//!   Up/k        - Increase WPM by 50
//!   Down/j      - Decrease WPM by 50
//!   Left/h      - Go back 1 word
//!   Right/l     - Go forward 1 word
//!   [/b         - Go back 10 words
//!   ]/w         - Go forward 10 words
//!   r           - Reset to beginning
//!   o           - Open library
//!   i           - Import file
//!   d           - Delete current book
//!   ?           - Show help
//!   q/Escape    - Quit

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::{self, stdout},
    path::PathBuf,
    time::{Duration, Instant},
};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Book {
    id: String,
    title: String,
    original_path: String,
    total_words: usize,
    progress: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    wpm: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { wpm: 300 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Library {
    books: Vec<Book>,
    last_book: Option<String>,
    settings: Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AppMode {
    Reading,
    Library,
    FileInput,
    Help,
    Confirm,
}

struct App {
    mode: AppMode,
    library: Library,
    words: Vec<String>,
    word_index: usize,
    current_book_id: Option<String>,
    current_book_title: String,
    is_playing: bool,
    wpm: u32,
    last_advance: Instant,

    // Library browser state
    library_state: ListState,

    // File input state
    file_input: String,
    file_input_cursor: usize,
    file_input_error: Option<String>,

    // Confirm dialog state
    confirm_message: String,
    confirm_action: Option<ConfirmAction>,

    // Status message
    status_message: Option<(String, Instant)>,
}

#[derive(Debug, Clone)]
enum ConfirmAction {
    DeleteBook(String),
}

// ============================================================================
// Configuration Paths
// ============================================================================

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rsvp-reader")
}

fn library_file() -> PathBuf {
    config_dir().join("library.json")
}

fn books_dir() -> PathBuf {
    config_dir().join("books")
}

fn ensure_config_dirs() -> io::Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::create_dir_all(books_dir())?;
    Ok(())
}

fn load_library() -> Library {
    if let Ok(content) = fs::read_to_string(library_file()) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Library::default()
    }
}

fn save_library(library: &Library) {
    let _ = ensure_config_dirs();
    if let Ok(content) = serde_json::to_string_pretty(library) {
        let _ = fs::write(library_file(), content);
    }
}

// ============================================================================
// Text Processing
// ============================================================================

fn tokenize_text(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_string()).collect()
}

/// Calculate the Optimal Recognition Point (ORP) for a word
fn calculate_orp(word: &str) -> usize {
    let len = word.chars().count();
    match len {
        0..=1 => 0,
        2..=5 => 1,
        6..=9 => 2,
        10..=13 => 3,
        _ => 4,
    }
}

// ============================================================================
// App Implementation
// ============================================================================

impl App {
    fn new() -> Self {
        let library = load_library();
        let wpm = library.settings.wpm;

        Self {
            mode: AppMode::Reading,
            library,
            words: Vec::new(),
            word_index: 0,
            current_book_id: None,
            current_book_title: String::new(),
            is_playing: false,
            wpm,
            last_advance: Instant::now(),
            library_state: ListState::default(),
            file_input: String::new(),
            file_input_cursor: 0,
            file_input_error: None,
            confirm_message: String::new(),
            confirm_action: None,
            status_message: None,
        }
    }

    fn show_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), Instant::now()));
    }

    fn load_last_book(&mut self) {
        if let Some(book_id) = self.library.last_book.clone() {
            self.load_book(&book_id);
        }
    }

    fn load_book(&mut self, book_id: &str) -> bool {
        let book_file = books_dir().join(format!("{}.txt", book_id));

        let content = match fs::read_to_string(&book_file) {
            Ok(c) => c,
            Err(_) => {
                self.show_status("Book file not found");
                return false;
            }
        };

        self.words = tokenize_text(&content);
        if self.words.is_empty() {
            self.show_status("Book is empty");
            return false;
        }

        // Find book info
        if let Some(book) = self.library.books.iter().find(|b| b.id == book_id) {
            self.current_book_title = book.title.clone();
            self.word_index = book.progress.min(self.words.len().saturating_sub(1));
        } else {
            self.current_book_title = "Unknown".to_string();
            self.word_index = 0;
        }

        self.current_book_id = Some(book_id.to_string());
        self.library.last_book = Some(book_id.to_string());
        save_library(&self.library);

        true
    }

    fn import_file(&mut self, path: &str) -> bool {
        let path = PathBuf::from(shellexpand(path));

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                self.file_input_error = Some(format!("Error: {}", e));
                return false;
            }
        };

        let words = tokenize_text(&content);
        if words.is_empty() {
            self.file_input_error = Some("File is empty".to_string());
            return false;
        }

        // Generate unique ID
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        std::time::SystemTime::now().hash(&mut hasher);
        let book_id = format!("{:x}", hasher.finish())[..12].to_string();

        // Save to books directory
        let _ = ensure_config_dirs();
        let book_file = books_dir().join(format!("{}.txt", book_id));
        if fs::write(&book_file, &content).is_err() {
            self.file_input_error = Some("Failed to save book".to_string());
            return false;
        }

        // Get title from filename
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Add to library
        let book = Book {
            id: book_id.clone(),
            title: title.clone(),
            original_path: path.to_string_lossy().to_string(),
            total_words: words.len(),
            progress: 0,
        };
        self.library.books.push(book);
        save_library(&self.library);

        self.show_status(&format!("Imported: {} ({} words)", title, words.len()));
        self.load_book(&book_id);

        true
    }

    fn save_progress(&mut self) {
        if let Some(ref book_id) = self.current_book_id {
            if let Some(book) = self.library.books.iter_mut().find(|b| b.id == *book_id) {
                book.progress = self.word_index;
            }
            save_library(&self.library);
        }
    }

    fn tick(&mut self) {
        // Clear old status messages
        if let Some((_, instant)) = &self.status_message {
            if instant.elapsed() > Duration::from_secs(3) {
                self.status_message = None;
            }
        }

        // Advance word if playing
        if self.is_playing && !self.words.is_empty() {
            let delay = Duration::from_secs_f64(60.0 / self.wpm as f64);
            if self.last_advance.elapsed() >= delay {
                self.last_advance = Instant::now();
                if self.word_index < self.words.len() - 1 {
                    self.word_index += 1;
                    // Save progress every 10 words
                    if self.word_index % 10 == 0 {
                        self.save_progress();
                    }
                } else {
                    self.is_playing = false;
                    self.show_status("Finished reading!");
                    self.save_progress();
                }
            }
        }
    }

    fn current_word(&self) -> Option<&str> {
        self.words.get(self.word_index).map(|s| s.as_str())
    }

    fn progress_percent(&self) -> f64 {
        if self.words.is_empty() {
            0.0
        } else {
            (self.word_index as f64 / self.words.len() as f64) * 100.0
        }
    }
}

fn shellexpand(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

// ============================================================================
// UI Rendering
// ============================================================================

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(10),   // Word display
            Constraint::Length(1), // Progress bar
            Constraint::Length(3), // Stats
        ])
        .split(size);

    // Title bar
    let title_text = if app.current_book_title.is_empty() {
        "Press 'i' to import a file or 'o' to open library"
    } else {
        &app.current_book_title
    };
    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::DarkGray)),
        );
    f.render_widget(title, chunks[0]);

    // Word display
    render_word_display(f, app, chunks[1]);

    // Progress bar
    let progress = app.progress_percent();
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Magenta)
                .bg(Color::DarkGray),
        )
        .percent(progress as u16)
        .label("");
    f.render_widget(gauge, chunks[2]);

    // Stats bar
    render_stats(f, app, chunks[3]);

    // Modal overlays
    match app.mode {
        AppMode::Library => render_library(f, app, size),
        AppMode::FileInput => render_file_input(f, app, size),
        AppMode::Help => render_help(f, size),
        AppMode::Confirm => render_confirm(f, app, size),
        _ => {}
    }
}

fn render_word_display(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" RSVP ")
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Calculate the center column position for the focal point
    let center_x = inner.x + inner.width / 2;
    let center_y = inner.y + inner.height / 2;

    // Draw fixed focal point markers (subtle vertical guides)
    let marker_style = Style::default().fg(Color::DarkGray);

    // Top marker
    if center_y > inner.y + 1 {
        let top_marker = Paragraph::new("|").style(marker_style);
        f.render_widget(top_marker, Rect::new(center_x, center_y - 2, 1, 1));
    }

    // Bottom marker
    if center_y + 2 < inner.y + inner.height {
        let bottom_marker = Paragraph::new("|").style(marker_style);
        f.render_widget(bottom_marker, Rect::new(center_x, center_y + 2, 1, 1));
    }

    if let Some(word) = app.current_word() {
        let orp = calculate_orp(word);
        let chars: Vec<char> = word.chars().collect();

        // Split word into three parts
        let before: String = chars[..orp].iter().collect();
        let orp_char: String = chars.get(orp).map(|c| c.to_string()).unwrap_or_default();
        let after: String = if orp + 1 < chars.len() {
            chars[orp + 1..].iter().collect()
        } else {
            String::new()
        };

        // ORP character is always at center_x
        // Render each part as a separate widget to avoid styling issues

        // Before ORP (right-aligned to center)
        if !before.is_empty() {
            let before_x = center_x.saturating_sub(before.len() as u16);
            let before_widget = Paragraph::new(before.clone())
                .style(Style::default().fg(Color::White));
            f.render_widget(before_widget, Rect::new(before_x, center_y, before.len() as u16, 1));
        }

        // ORP character (at center, in red)
        let orp_widget = Paragraph::new(orp_char.clone())
            .style(Style::default().fg(Color::Red));
        f.render_widget(orp_widget, Rect::new(center_x, center_y, 1, 1));

        // After ORP (left-aligned from center+1)
        if !after.is_empty() {
            let after_x = center_x + 1;
            let after_widget = Paragraph::new(after.clone())
                .style(Style::default().fg(Color::White));
            f.render_widget(after_widget, Rect::new(after_x, center_y, after.len() as u16, 1));
        }
    } else {
        let text = Paragraph::new("Ready")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);

        let centered = Rect::new(inner.x, center_y, inner.width, 1);
        f.render_widget(text, centered);
    }
}

fn render_stats(f: &mut Frame, app: &App, area: Rect) {
    let status = if app.is_playing {
        "Playing"
    } else {
        "Paused"
    };
    let status_color = if app.is_playing {
        Color::Green
    } else {
        Color::Yellow
    };

    let stats_text = Line::from(vec![
        Span::styled(
            format!("WPM: {} ", app.wpm),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("| "),
        Span::styled(
            format!("Word: {}/{} ", app.word_index + 1, app.words.len().max(1)),
            Style::default().fg(Color::Blue),
        ),
        Span::raw("| "),
        Span::styled(
            format!("Progress: {:.1}% ", app.progress_percent()),
            Style::default().fg(Color::Magenta),
        ),
        Span::raw("| "),
        Span::styled(
            status,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        if let Some((msg, _)) = &app.status_message {
            Span::styled(format!(" | {}", msg), Style::default().fg(Color::Yellow))
        } else {
            Span::raw("")
        },
    ]);

    let stats = Paragraph::new(stats_text).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    f.render_widget(stats, area);
}

fn render_library(f: &mut Frame, app: &App, size: Rect) {
    let area = centered_rect(60, 70, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Library ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.library.books.is_empty() {
        let text = Paragraph::new("No books in library.\n\nPress 'i' to import a file.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(text, inner);
    } else {
        let items: Vec<ListItem> = app
            .library
            .books
            .iter()
            .map(|book| {
                let marker = if Some(&book.id) == app.current_book_id.as_ref() {
                    "> "
                } else {
                    "  "
                };
                let pct = if book.total_words > 0 {
                    (book.progress as f64 / book.total_words as f64) * 100.0
                } else {
                    0.0
                };
                let line = Line::from(vec![
                    Span::styled(marker, Style::default().fg(Color::Green)),
                    Span::styled(
                        &book.title,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" ({:.0}% - {} words)", pct, book.total_words),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("-> ");

        // Need to render with state for highlighting
        let mut state = app.library_state.clone();
        f.render_stateful_widget(list, inner, &mut state);
    }

    // Help text at bottom
    let help_area = Rect::new(area.x + 1, area.y + area.height - 2, area.width - 2, 1);
    let help = Paragraph::new("Enter: Open | d: Delete | Esc: Close")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, help_area);
}

fn render_file_input(f: &mut Frame, app: &App, size: Rect) {
    let area = centered_rect(70, 30, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Import File ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    // Label
    let label = Paragraph::new("Enter file path:").style(Style::default().fg(Color::White));
    f.render_widget(label, chunks[0]);

    // Input field
    let input_style = if app.file_input_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::White)
    };
    let input = Paragraph::new(app.file_input.as_str()).style(input_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    f.render_widget(input, chunks[1]);

    // Show cursor
    let cursor_x = chunks[1].x + 1 + app.file_input_cursor as u16;
    let cursor_y = chunks[1].y + 1;
    f.set_cursor_position((cursor_x.min(chunks[1].x + chunks[1].width - 2), cursor_y));

    // Error message
    if let Some(ref error) = app.file_input_error {
        let error_text = Paragraph::new(error.as_str()).style(Style::default().fg(Color::Red));
        f.render_widget(error_text, chunks[2]);
    }

    // Help
    let help = Paragraph::new("Enter: Import | Esc: Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[3]);
}

fn render_help(f: &mut Frame, size: Rect) {
    let area = centered_rect(60, 80, size);
    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(Span::styled(
            "RSVP Reader - Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Playback Controls:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  Space      ", Style::default().fg(Color::Green)),
            Span::raw("Start/Pause reading"),
        ]),
        Line::from(vec![
            Span::styled("  r          ", Style::default().fg(Color::Green)),
            Span::raw("Reset to beginning"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Speed Control:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  Up / k     ", Style::default().fg(Color::Green)),
            Span::raw("Increase WPM by 50"),
        ]),
        Line::from(vec![
            Span::styled("  Down / j   ", Style::default().fg(Color::Green)),
            Span::raw("Decrease WPM by 50"),
        ]),
        Line::from(vec![
            Span::styled("  Shift+Up   ", Style::default().fg(Color::Green)),
            Span::raw("Increase WPM by 100"),
        ]),
        Line::from(vec![
            Span::styled("  Shift+Down ", Style::default().fg(Color::Green)),
            Span::raw("Decrease WPM by 100"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Navigation:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  Left / h   ", Style::default().fg(Color::Green)),
            Span::raw("Go back 1 word"),
        ]),
        Line::from(vec![
            Span::styled("  Right / l  ", Style::default().fg(Color::Green)),
            Span::raw("Go forward 1 word"),
        ]),
        Line::from(vec![
            Span::styled("  [ / b      ", Style::default().fg(Color::Green)),
            Span::raw("Go back 10 words"),
        ]),
        Line::from(vec![
            Span::styled("  ] / w      ", Style::default().fg(Color::Green)),
            Span::raw("Go forward 10 words"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Library:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  o          ", Style::default().fg(Color::Green)),
            Span::raw("Open library"),
        ]),
        Line::from(vec![
            Span::styled("  i          ", Style::default().fg(Color::Green)),
            Span::raw("Import new file"),
        ]),
        Line::from(vec![
            Span::styled("  d          ", Style::default().fg(Color::Green)),
            Span::raw("Delete current book"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Other:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  ?          ", Style::default().fg(Color::Green)),
            Span::raw("Show this help"),
        ]),
        Line::from(vec![
            Span::styled("  q / Esc    ", Style::default().fg(Color::Green)),
            Span::raw("Quit"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn render_confirm(f: &mut Frame, app: &App, size: Rect) {
    let area = centered_rect(50, 20, size);
    f.render_widget(Clear, area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            &app.confirm_message,
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Yes  "),
            Span::styled(
                "n",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": No"),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Confirm ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ============================================================================
// Event Handling
// ============================================================================

fn handle_events(app: &mut App) -> io::Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            match app.mode {
                AppMode::Reading => return handle_reading_keys(app, key.code, key.modifiers),
                AppMode::Library => handle_library_keys(app, key.code),
                AppMode::FileInput => handle_file_input_keys(app, key.code),
                AppMode::Help => app.mode = AppMode::Reading,
                AppMode::Confirm => handle_confirm_keys(app, key.code),
            }
        }
    }
    Ok(false)
}

fn handle_reading_keys(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> io::Result<bool> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Char(' ') => {
            if !app.words.is_empty() {
                if app.word_index >= app.words.len() - 1 {
                    app.word_index = 0;
                }
                app.is_playing = !app.is_playing;
                app.last_advance = Instant::now();
            } else {
                app.show_status("No book loaded. Press 'i' to import.");
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let increment = if modifiers.contains(KeyModifiers::SHIFT) {
                100
            } else {
                50
            };
            app.wpm = (app.wpm + increment).min(2000);
            app.library.settings.wpm = app.wpm;
            save_library(&app.library);
            app.show_status(&format!("Speed: {} WPM", app.wpm));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let decrement = if modifiers.contains(KeyModifiers::SHIFT) {
                100
            } else {
                50
            };
            app.wpm = app.wpm.saturating_sub(decrement).max(50);
            app.library.settings.wpm = app.wpm;
            save_library(&app.library);
            app.show_status(&format!("Speed: {} WPM", app.wpm));
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.is_playing = false;
            app.word_index = app.word_index.saturating_sub(1);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.is_playing = false;
            if !app.words.is_empty() {
                app.word_index = (app.word_index + 1).min(app.words.len() - 1);
            }
        }
        KeyCode::Char('[') | KeyCode::Char('b') => {
            app.is_playing = false;
            app.word_index = app.word_index.saturating_sub(10);
        }
        KeyCode::Char(']') | KeyCode::Char('w') => {
            app.is_playing = false;
            if !app.words.is_empty() {
                app.word_index = (app.word_index + 10).min(app.words.len() - 1);
            }
        }
        KeyCode::Char('r') => {
            app.is_playing = false;
            app.word_index = 0;
            app.save_progress();
            app.show_status("Reset to beginning");
        }
        KeyCode::Char('o') => {
            app.is_playing = false;
            app.mode = AppMode::Library;
            if !app.library.books.is_empty() {
                app.library_state.select(Some(0));
            }
        }
        KeyCode::Char('i') => {
            app.is_playing = false;
            app.mode = AppMode::FileInput;
            app.file_input.clear();
            app.file_input_cursor = 0;
            app.file_input_error = None;
        }
        KeyCode::Char('d') => {
            if app.current_book_id.is_some() {
                app.is_playing = false;
                app.confirm_message = format!("Delete '{}'?", app.current_book_title);
                app.confirm_action = Some(ConfirmAction::DeleteBook(
                    app.current_book_id.clone().unwrap(),
                ));
                app.mode = AppMode::Confirm;
            } else {
                app.show_status("No book loaded");
            }
        }
        KeyCode::Char('?') => {
            app.is_playing = false;
            app.mode = AppMode::Help;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_library_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.mode = AppMode::Reading;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !app.library.books.is_empty() {
                let i = app.library_state.selected().unwrap_or(0);
                let new_i = if i == 0 {
                    app.library.books.len() - 1
                } else {
                    i - 1
                };
                app.library_state.select(Some(new_i));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.library.books.is_empty() {
                let i = app.library_state.selected().unwrap_or(0);
                let new_i = (i + 1) % app.library.books.len();
                app.library_state.select(Some(new_i));
            }
        }
        KeyCode::Enter => {
            if let Some(i) = app.library_state.selected() {
                if let Some(book) = app.library.books.get(i) {
                    let book_id = book.id.clone();
                    app.load_book(&book_id);
                    app.mode = AppMode::Reading;
                }
            }
        }
        KeyCode::Char('d') => {
            if let Some(i) = app.library_state.selected() {
                if let Some(book) = app.library.books.get(i) {
                    app.confirm_message = format!("Delete '{}'?", book.title);
                    app.confirm_action = Some(ConfirmAction::DeleteBook(book.id.clone()));
                    app.mode = AppMode::Confirm;
                }
            }
        }
        KeyCode::Char('i') => {
            app.mode = AppMode::FileInput;
            app.file_input.clear();
            app.file_input_cursor = 0;
            app.file_input_error = None;
        }
        _ => {}
    }
}

fn handle_file_input_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.mode = AppMode::Reading;
        }
        KeyCode::Enter => {
            if !app.file_input.is_empty() {
                let path = app.file_input.clone();
                if app.import_file(&path) {
                    app.mode = AppMode::Reading;
                }
            }
        }
        KeyCode::Char(c) => {
            app.file_input.insert(app.file_input_cursor, c);
            app.file_input_cursor += 1;
            app.file_input_error = None;
        }
        KeyCode::Backspace => {
            if app.file_input_cursor > 0 {
                app.file_input_cursor -= 1;
                app.file_input.remove(app.file_input_cursor);
                app.file_input_error = None;
            }
        }
        KeyCode::Delete => {
            if app.file_input_cursor < app.file_input.len() {
                app.file_input.remove(app.file_input_cursor);
                app.file_input_error = None;
            }
        }
        KeyCode::Left => {
            app.file_input_cursor = app.file_input_cursor.saturating_sub(1);
        }
        KeyCode::Right => {
            app.file_input_cursor = (app.file_input_cursor + 1).min(app.file_input.len());
        }
        KeyCode::Home => {
            app.file_input_cursor = 0;
        }
        KeyCode::End => {
            app.file_input_cursor = app.file_input.len();
        }
        _ => {}
    }
}

fn handle_confirm_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Some(action) = app.confirm_action.take() {
                match action {
                    ConfirmAction::DeleteBook(book_id) => {
                        // Check if we're deleting the current book
                        let is_current = app.current_book_id.as_ref() == Some(&book_id);

                        // Get the title for the message
                        let title = app
                            .library
                            .books
                            .iter()
                            .find(|b| b.id == book_id)
                            .map(|b| b.title.clone())
                            .unwrap_or_default();

                        // Remove from library
                        app.library.books.retain(|b| b.id != book_id);
                        if app.library.last_book.as_ref() == Some(&book_id) {
                            app.library.last_book = None;
                        }
                        save_library(&app.library);

                        // Remove file
                        let book_file = books_dir().join(format!("{}.txt", book_id));
                        let _ = fs::remove_file(book_file);

                        // Reset state if we deleted the current book
                        if is_current {
                            app.words.clear();
                            app.current_book_id = None;
                            app.current_book_title.clear();
                            app.word_index = 0;
                            app.is_playing = false;
                        }

                        app.show_status(&format!("Deleted: {}", title));
                    }
                }
            }
            app.mode = AppMode::Reading;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.confirm_action = None;
            app.mode = AppMode::Reading;
        }
        _ => {}
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and load last book
    let mut app = App::new();
    app.load_last_book();

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Save progress before exit
    app.save_progress();

    result
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        app.tick();

        if handle_events(app)? {
            break;
        }
    }
    Ok(())
}
