//! RSVP Reader - Desktop GUI for Rapid Serial Visual Presentation speed reading
//!
//! Keyboard shortcuts:
//!   Space       - Start/Pause
//!   Up/Down     - Adjust WPM
//!   Left/Right  - Navigate words
//!   R           - Reset
//!   O           - Open file
//!   Escape      - Quit

use iced::keyboard::{self, Key};
use iced::theme::{self, Theme};
use iced::time;
use iced::widget::{button, column, container, row, text, Space};
use iced::{executor, Application, Color, Command, Element, Font, Length, Settings, Subscription};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ============================================================================
// Configuration
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

fn ensure_config_dirs() -> std::io::Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::create_dir_all(books_dir())?;
    Ok(())
}

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Book {
    id: String,
    title: String,
    total_words: usize,
    progress: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Library {
    books: Vec<Book>,
    last_book: Option<String>,
    wpm: u32,
}

fn load_library() -> Library {
    if let Ok(content) = fs::read_to_string(library_file()) {
        serde_json::from_str(&content).unwrap_or_else(|_| Library {
            wpm: 300,
            ..Default::default()
        })
    } else {
        Library {
            wpm: 300,
            ..Default::default()
        }
    }
}

fn save_library(library: &Library) {
    let _ = ensure_config_dirs();
    if let Ok(content) = serde_json::to_string_pretty(library) {
        let _ = fs::write(library_file(), content);
    }
}

// ============================================================================
// ORP Calculation
// ============================================================================

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

fn tokenize_text(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_string()).collect()
}

// ============================================================================
// Application
// ============================================================================

pub fn main() -> iced::Result {
    RSVPApp::run(Settings {
        window: iced::window::Settings {
            size: iced::Size::new(800.0, 500.0),
            min_size: Some(iced::Size::new(600.0, 400.0)),
            ..Default::default()
        },
        antialiasing: true,
        ..Default::default()
    })
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    TogglePlay,
    SpeedUp,
    SpeedDown,
    PrevWord,
    NextWord,
    Reset,
    OpenFile,
    FileOpened(Option<PathBuf>),
    KeyPressed(Key),
}

struct RSVPApp {
    library: Library,
    words: Vec<String>,
    word_index: usize,
    current_book_id: Option<String>,
    current_book_title: String,
    is_playing: bool,
    wpm: u32,
    last_tick: Instant,
    status_message: Option<String>,
}

impl Application for RSVPApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let library = load_library();
        let wpm = if library.wpm > 0 { library.wpm } else { 300 };

        let mut app = Self {
            library,
            words: Vec::new(),
            word_index: 0,
            current_book_id: None,
            current_book_title: String::new(),
            is_playing: false,
            wpm,
            last_tick: Instant::now(),
            status_message: Some("Press O to open a file, Space to play/pause".to_string()),
        };

        // Load last book if available
        if let Some(book_id) = app.library.last_book.clone() {
            app.load_book(&book_id);
        }

        (app, Command::none())
    }

    fn title(&self) -> String {
        if self.current_book_title.is_empty() {
            "RSVP Reader".to_string()
        } else {
            format!("RSVP Reader - {}", self.current_book_title)
        }
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Tick => {
                if self.is_playing && !self.words.is_empty() {
                    let delay = Duration::from_secs_f64(60.0 / self.wpm as f64);
                    if self.last_tick.elapsed() >= delay {
                        self.last_tick = Instant::now();
                        if self.word_index < self.words.len() - 1 {
                            self.word_index += 1;
                            if self.word_index % 10 == 0 {
                                self.save_progress();
                            }
                        } else {
                            self.is_playing = false;
                            self.status_message = Some("Finished!".to_string());
                            self.save_progress();
                        }
                    }
                }
            }
            Message::TogglePlay => {
                if !self.words.is_empty() {
                    if self.word_index >= self.words.len() - 1 {
                        self.word_index = 0;
                    }
                    self.is_playing = !self.is_playing;
                    self.last_tick = Instant::now();
                    self.status_message = None;
                }
            }
            Message::SpeedUp => {
                self.wpm = (self.wpm + 50).min(2000);
                self.library.wpm = self.wpm;
                save_library(&self.library);
                self.status_message = Some(format!("{} WPM", self.wpm));
            }
            Message::SpeedDown => {
                self.wpm = self.wpm.saturating_sub(50).max(50);
                self.library.wpm = self.wpm;
                save_library(&self.library);
                self.status_message = Some(format!("{} WPM", self.wpm));
            }
            Message::PrevWord => {
                self.is_playing = false;
                self.word_index = self.word_index.saturating_sub(1);
            }
            Message::NextWord => {
                self.is_playing = false;
                if !self.words.is_empty() {
                    self.word_index = (self.word_index + 1).min(self.words.len() - 1);
                }
            }
            Message::Reset => {
                self.is_playing = false;
                self.word_index = 0;
                self.save_progress();
                self.status_message = Some("Reset to beginning".to_string());
            }
            Message::OpenFile => {
                return Command::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Text files", &["txt", "md", "text"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::FileOpened,
                );
            }
            Message::FileOpened(path) => {
                if let Some(path) = path {
                    self.import_file(&path);
                }
            }
            Message::KeyPressed(key) => match key.as_ref() {
                Key::Named(keyboard::key::Named::Space) => {
                    return self.update(Message::TogglePlay);
                }
                Key::Named(keyboard::key::Named::ArrowUp) => {
                    return self.update(Message::SpeedUp);
                }
                Key::Named(keyboard::key::Named::ArrowDown) => {
                    return self.update(Message::SpeedDown);
                }
                Key::Named(keyboard::key::Named::ArrowLeft) => {
                    return self.update(Message::PrevWord);
                }
                Key::Named(keyboard::key::Named::ArrowRight) => {
                    return self.update(Message::NextWord);
                }
                Key::Named(keyboard::key::Named::Escape) => {
                    std::process::exit(0);
                }
                Key::Character(c) => {
                    let s: &str = c.as_ref();
                    match s {
                        "r" | "R" => return self.update(Message::Reset),
                        "o" | "O" => return self.update(Message::OpenFile),
                        " " => return self.update(Message::TogglePlay),
                        _ => {}
                    }
                }
                _ => {}
            },
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let progress = if self.words.is_empty() {
            0.0
        } else {
            (self.word_index as f32 / self.words.len() as f32) * 100.0
        };

        // Word display with ORP highlighting
        let word_display: Element<Message> = if let Some(word) = self.words.get(self.word_index) {
            let orp = calculate_orp(word);
            let chars: Vec<char> = word.chars().collect();

            let mut word_row = row![];

            for (i, ch) in chars.iter().enumerate() {
                let color = if i == orp {
                    Color::from_rgb(0.9, 0.2, 0.2) // Red for ORP
                } else {
                    Color::from_rgb(0.9, 0.9, 0.9) // White for others
                };

                word_row = word_row.push(
                    text(ch.to_string())
                        .size(72)
                        .style(color)
                        .font(Font::DEFAULT),
                );
            }

            container(word_row)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x()
                .center_y()
                .into()
        } else {
            container(
                text("Press O to open a file")
                    .size(32)
                    .style(Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
        };

        // Stats bar
        let status = if self.is_playing { "▶ Playing" } else { "⏸ Paused" };
        let stats_text = format!(
            "WPM: {}  │  Word: {}/{}  │  Progress: {:.1}%  │  {}",
            self.wpm,
            self.word_index + 1,
            self.words.len().max(1),
            progress,
            status
        );

        let stats_bar = container(
            text(stats_text)
                .size(16)
                .style(Color::from_rgb(0.6, 0.6, 0.6)),
        )
        .width(Length::Fill)
        .padding(10)
        .center_x();

        // Control buttons
        let controls = row![
            button(text("◀◀").size(20)).on_press(Message::Reset).padding(10),
            button(text("◀").size(20)).on_press(Message::PrevWord).padding(10),
            button(text(if self.is_playing { "⏸" } else { "▶" }).size(20))
                .on_press(Message::TogglePlay)
                .padding(10),
            button(text("▶").size(20)).on_press(Message::NextWord).padding(10),
            Space::with_width(20),
            button(text("−").size(20)).on_press(Message::SpeedDown).padding(10),
            text(format!("{} WPM", self.wpm)).size(16),
            button(text("+").size(20)).on_press(Message::SpeedUp).padding(10),
            Space::with_width(20),
            button(text("Open").size(16)).on_press(Message::OpenFile).padding(10),
        ]
        .spacing(10)
        .align_items(iced::Alignment::Center);

        let controls_bar = container(controls)
            .width(Length::Fill)
            .padding(15)
            .center_x();

        // Status message
        let status_bar = if let Some(msg) = &self.status_message {
            container(text(msg).size(14).style(Color::from_rgb(0.7, 0.7, 0.3)))
                .width(Length::Fill)
                .padding(5)
                .center_x()
        } else {
            container(text("").size(14))
                .width(Length::Fill)
                .padding(5)
        };

        // Main layout
        let content = column![
            stats_bar,
            word_display,
            controls_bar,
            status_bar,
        ]
        .spacing(0);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::Container::Custom(Box::new(DarkContainer)))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = if self.is_playing {
            time::every(Duration::from_millis(10)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        };

        let keys = keyboard::on_key_press(|key, _modifiers| Some(Message::KeyPressed(key)));

        Subscription::batch([tick, keys])
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

impl RSVPApp {
    fn load_book(&mut self, book_id: &str) -> bool {
        let book_file = books_dir().join(format!("{}.txt", book_id));

        let content = match fs::read_to_string(&book_file) {
            Ok(c) => c,
            Err(_) => return false,
        };

        self.words = tokenize_text(&content);
        if self.words.is_empty() {
            return false;
        }

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

    fn import_file(&mut self, path: &PathBuf) -> bool {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                self.status_message = Some(format!("Error: {}", e));
                return false;
            }
        };

        let words = tokenize_text(&content);
        if words.is_empty() {
            self.status_message = Some("File is empty".to_string());
            return false;
        }

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        std::time::SystemTime::now().hash(&mut hasher);
        let book_id = format!("{:x}", hasher.finish())[..12].to_string();

        let _ = ensure_config_dirs();
        let book_file = books_dir().join(format!("{}.txt", book_id));
        if fs::write(&book_file, &content).is_err() {
            self.status_message = Some("Failed to save book".to_string());
            return false;
        }

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let book = Book {
            id: book_id.clone(),
            title: title.clone(),
            total_words: words.len(),
            progress: 0,
        };
        self.library.books.push(book);
        save_library(&self.library);

        self.status_message = Some(format!("Loaded: {} ({} words)", title, words.len()));
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
}

// Custom dark container style
struct DarkContainer;

impl container::StyleSheet for DarkContainer {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(iced::Background::Color(Color::from_rgb(0.1, 0.1, 0.12))),
            text_color: Some(Color::WHITE),
            ..Default::default()
        }
    }
}
