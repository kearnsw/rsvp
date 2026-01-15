#!/usr/bin/env python3
"""
RSVP Reader - Terminal-based Rapid Serial Visual Presentation speed reader.

Hotkeys:
    Space       - Start/Pause reading
    Up/k        - Increase WPM by 50
    Down/j      - Decrease WPM by 50
    Left/h      - Go back 1 word
    Right/l     - Go forward 1 word
    [           - Go back 10 words
    ]           - Go forward 10 words
    r           - Reset to beginning
    o           - Open library
    i           - Import file
    d           - Delete current book
    q/Escape    - Quit
"""

import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Optional

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical, Center
from textual.screen import Screen, ModalScreen
from textual.widgets import (
    Button,
    Footer,
    Header,
    Label,
    ListItem,
    ListView,
    Static,
    Input,
    DirectoryTree,
    ProgressBar,
)
from textual.reactive import reactive
from rich.text import Text
from rich.align import Align
from rich.panel import Panel


# Configuration paths
CONFIG_DIR = Path.home() / ".config" / "rsvp-reader"
LIBRARY_FILE = CONFIG_DIR / "library.json"
BOOKS_DIR = CONFIG_DIR / "books"


def ensure_config_dirs():
    """Ensure configuration directories exist."""
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    BOOKS_DIR.mkdir(parents=True, exist_ok=True)


def load_library() -> dict:
    """Load the book library from disk."""
    if LIBRARY_FILE.exists():
        try:
            with open(LIBRARY_FILE, "r") as f:
                return json.load(f)
        except (json.JSONDecodeError, IOError):
            return {"books": [], "last_book": None, "settings": {"wpm": 300}}
    return {"books": [], "last_book": None, "settings": {"wpm": 300}}


def save_library(library: dict):
    """Save the book library to disk."""
    ensure_config_dirs()
    with open(LIBRARY_FILE, "w") as f:
        json.dump(library, f, indent=2)


def tokenize_text(text: str) -> list[str]:
    """Split text into words for RSVP display."""
    # Split on whitespace and filter empty strings
    words = re.split(r'\s+', text.strip())
    return [w for w in words if w]


def calculate_focal_point(word: str) -> int:
    """
    Calculate the optimal focal point (ORP - Optimal Recognition Point) for a word.

    Research suggests the ORP is typically:
    - 1 char words: position 0
    - 2-5 char words: position 1
    - 6-9 char words: position 2
    - 10-13 char words: position 3
    - 14+ char words: position 4
    """
    length = len(word)
    if length <= 1:
        return 0
    elif length <= 5:
        return 1
    elif length <= 9:
        return 2
    elif length <= 13:
        return 3
    else:
        return 4


class WordDisplay(Static):
    """Widget to display the current word with focal point highlighting."""

    word = reactive("")

    def render(self) -> Text:
        if not self.word:
            return Text("Ready", style="dim italic", justify="center")

        focal_idx = calculate_focal_point(self.word)
        text = Text(justify="center")

        for i, char in enumerate(self.word):
            if i == focal_idx:
                text.append(char, style="bold red")
            else:
                text.append(char, style="bold white")

        return text


class StatsPanel(Static):
    """Widget to display reading statistics."""

    wpm = reactive(300)
    word_index = reactive(0)
    total_words = reactive(0)
    is_playing = reactive(False)

    def render(self) -> Text:
        progress = (self.word_index / self.total_words * 100) if self.total_words > 0 else 0
        status = "[green]Playing[/]" if self.is_playing else "[yellow]Paused[/]"

        text = Text()
        text.append(f"WPM: {self.wpm}  |  ", style="cyan")
        text.append(f"Word: {self.word_index}/{self.total_words}  |  ", style="blue")
        text.append(f"Progress: {progress:.1f}%  |  ", style="magenta")
        text.append(f"Status: ")
        if self.is_playing:
            text.append("Playing", style="green bold")
        else:
            text.append("Paused", style="yellow")

        return text


class HelpScreen(ModalScreen):
    """Modal screen showing keyboard shortcuts."""

    BINDINGS = [
        Binding("escape", "dismiss", "Close"),
        Binding("q", "dismiss", "Close"),
    ]

    def compose(self) -> ComposeResult:
        help_text = """
[bold cyan]RSVP Reader - Keyboard Shortcuts[/]

[bold]Playback Controls:[/]
  [green]Space[/]         Start/Pause reading
  [green]r[/]             Reset to beginning

[bold]Speed Control:[/]
  [green]Up / k[/]        Increase WPM by 50
  [green]Down / j[/]      Decrease WPM by 50
  [green]Shift+Up[/]      Increase WPM by 100
  [green]Shift+Down[/]    Decrease WPM by 100

[bold]Navigation:[/]
  [green]Left / h[/]      Go back 1 word
  [green]Right / l[/]     Go forward 1 word
  [green][ / b[/]         Go back 10 words
  [green]] / w[/]         Go forward 10 words
  [green]Home[/]          Go to beginning
  [green]End[/]           Go to end

[bold]Library:[/]
  [green]o[/]             Open library
  [green]i[/]             Import new file
  [green]d[/]             Delete current book

[bold]Other:[/]
  [green]?[/]             Show this help
  [green]q / Escape[/]    Quit

[dim]Press Escape or Q to close this help[/]
"""
        yield Container(
            Static(help_text, id="help-content"),
            id="help-dialog",
        )

    def on_click(self, event):
        self.dismiss()


class FilePickerScreen(ModalScreen[Optional[str]]):
    """Modal screen for selecting a file to import."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
    ]

    def __init__(self, start_path: str = "."):
        super().__init__()
        self.start_path = start_path

    def compose(self) -> ComposeResult:
        yield Container(
            Static("[bold]Select a text file to import[/]", id="picker-title"),
            Input(placeholder="Or paste/type file path here...", id="path-input"),
            DirectoryTree(self.start_path, id="file-tree"),
            Horizontal(
                Button("Import", variant="primary", id="import-btn"),
                Button("Cancel", variant="default", id="cancel-btn"),
                id="picker-buttons",
            ),
            id="file-picker-dialog",
        )

    @on(DirectoryTree.FileSelected)
    def on_file_selected(self, event: DirectoryTree.FileSelected):
        self.query_one("#path-input", Input).value = str(event.path)

    @on(Button.Pressed, "#import-btn")
    def on_import(self):
        path = self.query_one("#path-input", Input).value
        if path:
            self.dismiss(path)
        else:
            self.notify("Please select or enter a file path", severity="warning")

    @on(Button.Pressed, "#cancel-btn")
    def action_cancel(self):
        self.dismiss(None)


class LibraryScreen(ModalScreen[Optional[str]]):
    """Modal screen for managing the book library."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
        Binding("d", "delete", "Delete"),
    ]

    def __init__(self, library: dict, current_book: Optional[str] = None):
        super().__init__()
        self.library = library
        self.current_book = current_book

    def compose(self) -> ComposeResult:
        yield Container(
            Static("[bold cyan]Library[/]", id="library-title"),
            ListView(id="book-list"),
            Horizontal(
                Button("Open", variant="primary", id="open-btn"),
                Button("Delete", variant="error", id="delete-btn"),
                Button("Cancel", variant="default", id="cancel-btn"),
                id="library-buttons",
            ),
            id="library-dialog",
        )

    def on_mount(self):
        self._refresh_list()

    def _refresh_list(self):
        list_view = self.query_one("#book-list", ListView)
        list_view.clear()

        if not self.library.get("books"):
            list_view.append(ListItem(Label("[dim]No books in library. Press 'i' to import.[/]")))
            return

        for book in self.library["books"]:
            title = book.get("title", "Unknown")
            progress = book.get("progress", 0)
            total = book.get("total_words", 0)
            pct = (progress / total * 100) if total > 0 else 0

            marker = "[green]>[/] " if book.get("id") == self.current_book else "  "
            label = f"{marker}[bold]{title}[/] [dim]({pct:.0f}% - {total} words)[/]"
            list_view.append(ListItem(Label(label), id=f"book-{book.get('id', '')}"))

    def _get_selected_book_id(self) -> Optional[str]:
        list_view = self.query_one("#book-list", ListView)
        if list_view.highlighted_child:
            item_id = list_view.highlighted_child.id
            if item_id and item_id.startswith("book-"):
                return item_id[5:]
        return None

    @on(ListView.Selected)
    @on(Button.Pressed, "#open-btn")
    def on_open(self, event=None):
        book_id = self._get_selected_book_id()
        if book_id:
            self.dismiss(book_id)

    @on(Button.Pressed, "#delete-btn")
    def action_delete(self):
        book_id = self._get_selected_book_id()
        if book_id:
            # Remove from library
            self.library["books"] = [b for b in self.library["books"] if b.get("id") != book_id]
            save_library(self.library)

            # Remove file
            book_file = BOOKS_DIR / f"{book_id}.txt"
            if book_file.exists():
                book_file.unlink()

            self.notify(f"Book deleted", severity="information")
            self._refresh_list()

    @on(Button.Pressed, "#cancel-btn")
    def action_cancel(self):
        self.dismiss(None)


class RSVPApp(App):
    """Main RSVP Reader application."""

    CSS = """
    Screen {
        background: $surface;
    }

    #main-container {
        height: 100%;
        width: 100%;
    }

    #title-bar {
        dock: top;
        height: 3;
        background: $primary-darken-2;
        content-align: center middle;
        text-style: bold;
    }

    #book-title {
        text-align: center;
        color: $text;
        padding: 1;
    }

    #word-container {
        height: 1fr;
        align: center middle;
        background: $surface-darken-1;
    }

    #word-display {
        width: 100%;
        height: auto;
        content-align: center middle;
        text-align: center;
        padding: 2 4;
    }

    #word-display Static {
        text-align: center;
        text-style: bold;
        color: $text;
    }

    WordDisplay {
        width: 100%;
        height: 5;
        content-align: center middle;
        text-align: center;
        text-style: bold;
        background: $surface-darken-2;
        border: solid $primary;
        padding: 1 2;
    }

    #stats-container {
        dock: bottom;
        height: 3;
        background: $primary-darken-3;
        content-align: center middle;
    }

    StatsPanel {
        width: 100%;
        text-align: center;
        padding: 1;
    }

    #progress-bar {
        dock: bottom;
        height: 1;
        margin: 0 1;
    }

    /* Modal dialogs */
    #help-dialog, #file-picker-dialog, #library-dialog {
        width: 70;
        height: auto;
        max-height: 80%;
        background: $surface;
        border: thick $primary;
        padding: 1 2;
    }

    #help-content {
        height: auto;
        padding: 1;
    }

    #picker-title, #library-title {
        text-align: center;
        padding: 1;
        text-style: bold;
    }

    #path-input {
        margin: 1 0;
    }

    #file-tree {
        height: 15;
        border: solid $primary-darken-2;
        margin: 1 0;
    }

    #book-list {
        height: 15;
        border: solid $primary-darken-2;
        margin: 1 0;
    }

    #picker-buttons, #library-buttons {
        align: center middle;
        height: auto;
        padding: 1 0;
    }

    #picker-buttons Button, #library-buttons Button {
        margin: 0 1;
    }

    ModalScreen {
        align: center middle;
    }

    /* Welcome message styling */
    #welcome-container {
        width: 100%;
        height: 100%;
        align: center middle;
    }

    #welcome-message {
        text-align: center;
        padding: 2;
    }
    """

    BINDINGS = [
        Binding("space", "toggle_play", "Play/Pause", show=True),
        Binding("up", "speed_up", "Speed +50"),
        Binding("k", "speed_up", "Speed +50", show=False),
        Binding("down", "speed_down", "Speed -50"),
        Binding("j", "speed_down", "Speed -50", show=False),
        Binding("shift+up", "speed_up_fast", "Speed +100", show=False),
        Binding("shift+down", "speed_down_fast", "Speed -100", show=False),
        Binding("left", "prev_word", "Prev Word"),
        Binding("h", "prev_word", "Prev Word", show=False),
        Binding("right", "next_word", "Next Word"),
        Binding("l", "next_word", "Next Word", show=False),
        Binding("bracketleft", "prev_10", "Back 10"),
        Binding("b", "prev_10", "Back 10", show=False),
        Binding("bracketright", "next_10", "Forward 10"),
        Binding("w", "next_10", "Forward 10", show=False),
        Binding("home", "go_start", "Start", show=False),
        Binding("end", "go_end", "End", show=False),
        Binding("r", "reset", "Reset"),
        Binding("o", "open_library", "Library", show=True),
        Binding("i", "import_file", "Import", show=True),
        Binding("d", "delete_book", "Delete"),
        Binding("question_mark", "show_help", "Help", show=True),
        Binding("q", "quit", "Quit", show=True),
        Binding("escape", "quit", "Quit", show=False),
    ]

    TITLE = "RSVP Reader"

    # Reactive state
    is_playing = reactive(False)
    wpm = reactive(300)
    word_index = reactive(0)

    def __init__(self):
        super().__init__()
        ensure_config_dirs()
        self.library = load_library()
        self.wpm = self.library.get("settings", {}).get("wpm", 300)
        self.words: list[str] = []
        self.current_book_id: Optional[str] = None
        self.current_book_title: str = "No book loaded"

    def compose(self) -> ComposeResult:
        yield Header()
        yield Container(
            Static(self.current_book_title, id="book-title"),
            id="title-bar",
        )
        yield Container(
            Center(
                WordDisplay(id="word-display"),
            ),
            id="word-container",
        )
        yield ProgressBar(total=100, show_eta=False, id="progress-bar")
        yield Container(
            StatsPanel(id="stats-panel"),
            id="stats-container",
        )
        yield Footer()

    def on_mount(self):
        """Initialize the app on mount."""
        self._update_stats()

        # Load last book if available
        last_book = self.library.get("last_book")
        if last_book:
            self._load_book(last_book)
        else:
            self._show_welcome()

    def _show_welcome(self):
        """Show welcome message when no book is loaded."""
        word_display = self.query_one("#word-display", WordDisplay)
        word_display.word = ""
        self.query_one("#book-title", Static).update(
            "[dim]Press [bold]i[/] to import a file or [bold]o[/] to open library[/]"
        )

    def _load_book(self, book_id: str) -> bool:
        """Load a book by its ID."""
        book_file = BOOKS_DIR / f"{book_id}.txt"

        if not book_file.exists():
            self.notify("Book file not found", severity="error")
            return False

        try:
            with open(book_file, "r", encoding="utf-8") as f:
                content = f.read()
        except IOError as e:
            self.notify(f"Error reading file: {e}", severity="error")
            return False

        self.words = tokenize_text(content)
        if not self.words:
            self.notify("Book is empty", severity="warning")
            return False

        # Find book in library
        book_info = next((b for b in self.library["books"] if b.get("id") == book_id), None)
        if book_info:
            self.current_book_title = book_info.get("title", "Unknown")
            self.word_index = book_info.get("progress", 0)
            # Ensure index is valid
            if self.word_index >= len(self.words):
                self.word_index = 0
        else:
            self.current_book_title = "Unknown"
            self.word_index = 0

        self.current_book_id = book_id
        self.library["last_book"] = book_id
        save_library(self.library)

        # Update UI
        self.query_one("#book-title", Static).update(f"[bold]{self.current_book_title}[/]")
        self._update_word_display()
        self._update_stats()

        return True

    def _import_file(self, file_path: str) -> bool:
        """Import a text file into the library."""
        path = Path(file_path).expanduser().resolve()

        if not path.exists():
            self.notify(f"File not found: {path}", severity="error")
            return False

        if not path.is_file():
            self.notify("Not a file", severity="error")
            return False

        try:
            with open(path, "r", encoding="utf-8") as f:
                content = f.read()
        except IOError as e:
            self.notify(f"Error reading file: {e}", severity="error")
            return False

        words = tokenize_text(content)
        if not words:
            self.notify("File is empty", severity="warning")
            return False

        # Generate unique ID
        import hashlib
        book_id = hashlib.sha256(f"{path.name}{time.time()}".encode()).hexdigest()[:12]

        # Save to books directory
        book_file = BOOKS_DIR / f"{book_id}.txt"
        with open(book_file, "w", encoding="utf-8") as f:
            f.write(content)

        # Add to library
        book_info = {
            "id": book_id,
            "title": path.stem,
            "original_path": str(path),
            "total_words": len(words),
            "progress": 0,
            "added": time.time(),
        }
        self.library["books"].append(book_info)
        save_library(self.library)

        self.notify(f"Imported: {path.stem} ({len(words)} words)")

        # Load the imported book
        self._load_book(book_id)

        return True

    def _save_progress(self):
        """Save current reading progress."""
        if self.current_book_id:
            for book in self.library["books"]:
                if book.get("id") == self.current_book_id:
                    book["progress"] = self.word_index
                    break
            save_library(self.library)

    def _update_word_display(self):
        """Update the word display widget."""
        word_display = self.query_one("#word-display", WordDisplay)
        if self.words and 0 <= self.word_index < len(self.words):
            word_display.word = self.words[self.word_index]
        else:
            word_display.word = ""

    def _update_stats(self):
        """Update the stats panel."""
        stats = self.query_one("#stats-panel", StatsPanel)
        stats.wpm = self.wpm
        stats.word_index = self.word_index
        stats.total_words = len(self.words)
        stats.is_playing = self.is_playing

        # Update progress bar
        progress_bar = self.query_one("#progress-bar", ProgressBar)
        if self.words:
            progress = (self.word_index / len(self.words)) * 100
            progress_bar.update(progress=progress)
        else:
            progress_bar.update(progress=0)

    def watch_is_playing(self, playing: bool):
        """React to play state changes."""
        self._update_stats()
        if playing:
            self._start_reading()

    def watch_word_index(self, index: int):
        """React to word index changes."""
        self._update_word_display()
        self._update_stats()

    def watch_wpm(self, wpm: int):
        """React to WPM changes."""
        self._update_stats()
        # Save WPM setting
        self.library["settings"]["wpm"] = wpm
        save_library(self.library)

    @work(exclusive=True, name="reader")
    async def _start_reading(self):
        """Background worker for advancing words."""
        import asyncio

        while self.is_playing and self.word_index < len(self.words):
            delay = 60.0 / self.wpm
            await asyncio.sleep(delay)

            if self.is_playing:  # Check again after sleep
                self.word_index += 1

                # Save progress periodically (every 10 words)
                if self.word_index % 10 == 0:
                    self._save_progress()

        if self.word_index >= len(self.words):
            self.is_playing = False
            self.notify("Finished reading!")
            self._save_progress()

    # Actions
    def action_toggle_play(self):
        """Toggle play/pause."""
        if not self.words:
            self.notify("No book loaded. Press 'i' to import.", severity="warning")
            return

        if self.word_index >= len(self.words):
            self.word_index = 0

        self.is_playing = not self.is_playing

    def action_speed_up(self):
        """Increase WPM by 50."""
        self.wpm = min(2000, self.wpm + 50)
        self.notify(f"Speed: {self.wpm} WPM")

    def action_speed_down(self):
        """Decrease WPM by 50."""
        self.wpm = max(50, self.wpm - 50)
        self.notify(f"Speed: {self.wpm} WPM")

    def action_speed_up_fast(self):
        """Increase WPM by 100."""
        self.wpm = min(2000, self.wpm + 100)
        self.notify(f"Speed: {self.wpm} WPM")

    def action_speed_down_fast(self):
        """Decrease WPM by 100."""
        self.wpm = max(50, self.wpm - 100)
        self.notify(f"Speed: {self.wpm} WPM")

    def action_prev_word(self):
        """Go to previous word."""
        was_playing = self.is_playing
        self.is_playing = False
        self.word_index = max(0, self.word_index - 1)
        if was_playing:
            self.is_playing = True

    def action_next_word(self):
        """Go to next word."""
        was_playing = self.is_playing
        self.is_playing = False
        if self.words:
            self.word_index = min(len(self.words) - 1, self.word_index + 1)
        if was_playing:
            self.is_playing = True

    def action_prev_10(self):
        """Go back 10 words."""
        was_playing = self.is_playing
        self.is_playing = False
        self.word_index = max(0, self.word_index - 10)
        if was_playing:
            self.is_playing = True

    def action_next_10(self):
        """Go forward 10 words."""
        was_playing = self.is_playing
        self.is_playing = False
        if self.words:
            self.word_index = min(len(self.words) - 1, self.word_index + 10)
        if was_playing:
            self.is_playing = True

    def action_go_start(self):
        """Go to start."""
        self.is_playing = False
        self.word_index = 0

    def action_go_end(self):
        """Go to end."""
        self.is_playing = False
        if self.words:
            self.word_index = len(self.words) - 1

    def action_reset(self):
        """Reset to beginning."""
        self.is_playing = False
        self.word_index = 0
        self._save_progress()
        self.notify("Reset to beginning")

    def action_show_help(self):
        """Show help screen."""
        self.push_screen(HelpScreen())

    async def action_open_library(self):
        """Open the library screen."""
        was_playing = self.is_playing
        self.is_playing = False

        result = await self.push_screen_wait(
            LibraryScreen(self.library, self.current_book_id)
        )

        if result:
            self._load_book(result)
        elif was_playing and self.words:
            self.is_playing = True

    async def action_import_file(self):
        """Import a new file."""
        was_playing = self.is_playing
        self.is_playing = False

        result = await self.push_screen_wait(
            FilePickerScreen(str(Path.home()))
        )

        if result:
            self._import_file(result)
        elif was_playing and self.words:
            self.is_playing = True

    def action_delete_book(self):
        """Delete the current book."""
        if not self.current_book_id:
            self.notify("No book loaded", severity="warning")
            return

        self.is_playing = False

        # Remove from library
        self.library["books"] = [
            b for b in self.library["books"]
            if b.get("id") != self.current_book_id
        ]
        self.library["last_book"] = None
        save_library(self.library)

        # Remove file
        book_file = BOOKS_DIR / f"{self.current_book_id}.txt"
        if book_file.exists():
            book_file.unlink()

        self.notify(f"Deleted: {self.current_book_title}")

        # Reset state
        self.words = []
        self.current_book_id = None
        self.current_book_title = "No book loaded"
        self.word_index = 0

        self._show_welcome()
        self._update_stats()

    def on_unmount(self):
        """Save state when app closes."""
        self._save_progress()


def main():
    """Entry point."""
    app = RSVPApp()
    app.run()


if __name__ == "__main__":
    main()
