# RSVP Reader

A terminal-based Rapid Serial Visual Presentation (RSVP) speed reader with a modern TUI interface.

RSVP is a technique that flashes words quickly in the same spot on the screen, eliminating eye movement (saccades) to increase reading speed and focus.

## Features

- **RSVP Display**: Words appear one at a time with the Optimal Recognition Point (ORP) highlighted in red
- **Adjustable Speed**: Control reading speed from 50 to 2000 WPM
- **Library Management**: Import, organize, and delete text files
- **Progress Tracking**: Automatically saves your position in each book
- **Keyboard-Driven**: Full hotkey support for efficient control

## Installation

```bash
pip install textual rich
```

Or install from the project:

```bash
pip install -e .
```

## Usage

```bash
python rsvp.py
```

Or if installed:

```bash
rsvp
```

## Keyboard Shortcuts

### Playback Controls
| Key | Action |
|-----|--------|
| `Space` | Start/Pause reading |
| `r` | Reset to beginning |

### Speed Control
| Key | Action |
|-----|--------|
| `Up` / `k` | Increase WPM by 50 |
| `Down` / `j` | Decrease WPM by 50 |
| `Shift+Up` | Increase WPM by 100 |
| `Shift+Down` | Decrease WPM by 100 |

### Navigation
| Key | Action |
|-----|--------|
| `Left` / `h` | Go back 1 word |
| `Right` / `l` | Go forward 1 word |
| `[` / `b` | Go back 10 words |
| `]` / `w` | Go forward 10 words |
| `Home` | Go to beginning |
| `End` | Go to end |

### Library Management
| Key | Action |
|-----|--------|
| `o` | Open library |
| `i` | Import new file |
| `d` | Delete current book |

### Other
| Key | Action |
|-----|--------|
| `?` | Show help |
| `q` / `Escape` | Quit |

## How It Works

### Optimal Recognition Point (ORP)

The ORP is the position in a word where your eye naturally fixates for fastest recognition. This reader highlights the ORP in red, making it easier for your brain to process each word quickly.

The ORP position is calculated based on word length:
- 1 character: position 0
- 2-5 characters: position 1
- 6-9 characters: position 2
- 10-13 characters: position 3
- 14+ characters: position 4

### Data Storage

Your library and settings are stored in `~/.config/rsvp-reader/`:
- `library.json` - Book metadata and settings
- `books/` - Imported text files

## Tips for Speed Reading

1. **Start Slow**: Begin at 200-300 WPM and gradually increase
2. **Stay Focused**: Find a quiet environment
3. **Don't Subvocalize**: Try not to "speak" the words in your head
4. **Practice Daily**: Even 10-15 minutes helps build the skill
5. **Take Breaks**: Rest your eyes every 20-30 minutes

## License

MIT
