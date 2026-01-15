# RSVP

A terminal-based speed reader using Rapid Serial Visual Presentation.

![Rust](https://img.shields.io/badge/rust-stable-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

## What is RSVP?

**Rapid Serial Visual Presentation (RSVP)** is a reading technique that displays text one word at a time in a fixed position on the screen. Instead of your eyes moving across lines of text, the words come to you.

### The Science

Traditional reading is surprisingly inefficient. Studies show that up to 80% of reading time is spent on **saccades**—the rapid eye movements between words—and **regressions**—backward glances to re-read text. These mechanical motions, not comprehension, are the bottleneck.

RSVP eliminates both:

- **No eye movement**: Words appear in one fixed location, so your eyes stay still
- **No regressions**: The stream moves forward, training your brain to comprehend on first pass
- **Optimal Recognition Point (ORP)**: Each word is positioned so your eye lands on the optimal character for instant recognition

Research on the ORP shows that we don't read words letter-by-letter. Instead, we recognize words by fixating on a specific point—typically slightly left of center. RSVP readers align each word to this point, highlighted in red, allowing near-instantaneous word recognition.

### Why It Works

When you read traditionally at 250 WPM, your eyes are actually only processing words for a fraction of that time—the rest is mechanical overhead. RSVP removes the overhead entirely.

Most people can comfortably RSVP read at **300-500 WPM** with full comprehension after brief practice. Some reach **800+ WPM**. Your brain is faster than your eyes; RSVP lets it prove it.

## Installation

### From Source

```bash
git clone https://github.com/kearnsw/rsvp.git
cd rsvp
cargo build --release
```

The binary will be at `target/release/rsvp`.

### Quick Start

```bash
# Run the reader
./target/release/rsvp

# Press 'i' to import a text file
# Press Space to start reading
```

## Features

- **Adjustable speed**: 50-2000 WPM with instant feedback
- **Progress tracking**: Automatically saves your position in each book
- **Library management**: Import, organize, and switch between multiple texts
- **Vim-style navigation**: `hjkl` keys, plus `[]` for jumping
- **Clean TUI**: Distraction-free reading with ratatui

## Controls

| Key | Action |
|-----|--------|
| `Space` | Play/Pause |
| `Up/k` | Increase speed (+50 WPM) |
| `Down/j` | Decrease speed (-50 WPM) |
| `Left/h` | Previous word |
| `Right/l` | Next word |
| `[` or `b` | Back 10 words |
| `]` or `w` | Forward 10 words |
| `r` | Reset to beginning |
| `o` | Open library |
| `i` | Import file |
| `d` | Delete current book |
| `?` | Help |
| `q` | Quit |

## Tips for Getting Started

1. **Start slow**: Begin at 250-300 WPM. Speed isn't the goal—comprehension is.
2. **Relax your eyes**: Let the words come to you. Don't try to "grab" them.
3. **Trust your brain**: Subvocalization (the inner voice) will fade naturally as you speed up.
4. **Short sessions**: 10-15 minutes at first. RSVP reading uses different mental muscles.
5. **Increase gradually**: Add 25-50 WPM once a speed feels effortless.

## Why Terminal?

Reading is focus. Terminals are focus. No notifications, no hyperlinks, no ads—just you and the text. The constraints are the feature.

## License

MIT
