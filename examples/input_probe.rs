//! Raw key-event probe. Not part of the game — a diagnostic for a human.
//!
//! Run it with `cargo run --example input_probe` and use it to answer the two
//! questions the input layer currently has to assume rather than measure:
//!
//! 1. Hold a letter key for three or more seconds. The `+` column is the gap
//!    since the previous event, so the first gap is the OS "delay until repeat"
//!    and the rest are the repeat interval.
//! 2. Hold a direction key, tap Space a few times, then release Space while
//!    still holding the direction. Watch whether the terminal ever resumes
//!    delivering events for the key that is still down, and how long it takes.
//!
//! The reported hold mode is the one the game itself would pick in this
//! terminal: Explicit terminals report real releases and none of the above
//! matters, Timeout terminals are where every event gap has to be guessed.
//!
//! Esc quits.

use std::io::{self, Write};
use std::time::Instant;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement};

use shelljump::terminal::KEYBOARD_ENHANCEMENT_FLAGS;

/// Restores the terminal however the probe exits, including on panic unwind.
struct RawGuard {
    enhanced: bool,
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        if self.enhanced {
            let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
    }
}

fn main() -> io::Result<()> {
    // Must be queried before raw mode's event stream starts: it round-trips
    // with the terminal.
    let enhanced = matches!(supports_keyboard_enhancement(), Ok(true));

    enable_raw_mode()?;
    let _guard = RawGuard { enhanced };

    let mut out = io::stdout();
    // Pushing the flags is the whole point: querying support only tells us the
    // terminal *can* report repeat and release events, and the mode line below
    // would claim Explicit while the kind column stayed stuck on "press".
    if enhanced {
        execute!(
            out,
            PushKeyboardEnhancementFlags(KEYBOARD_ENHANCEMENT_FLAGS)
        )?;
    }

    let mode = if enhanced { "Explicit" } else { "Timeout" };
    let flags = if enhanced {
        format!("{KEYBOARD_ENHANCEMENT_FLAGS:?}")
    } else {
        "none — this terminal does not support the protocol".to_owned()
    };
    write!(out, "hold mode in this terminal: {mode}\r\n")?;
    write!(out, "enhancement flags pushed: {flags}\r\n")?;
    write!(out, "elapsed  +gap     kind     key\r\n")?;
    out.flush()?;

    let epoch = Instant::now();
    let mut previous = 0.0;
    loop {
        let ev = event::read()?;
        let now = epoch.elapsed().as_secs_f64();
        let gap = now - previous;
        previous = now;

        match ev {
            Event::Key(key) => {
                let kind = match key.kind {
                    KeyEventKind::Press => "press",
                    KeyEventKind::Repeat => "repeat",
                    KeyEventKind::Release => "release",
                };
                write!(
                    out,
                    "{now:7.3}  {gap:6.3}   {kind:<8} {:?} {:?}\r\n",
                    key.code, key.modifiers
                )?;
                out.flush()?;
                if key.code == KeyCode::Esc {
                    return Ok(());
                }
            }
            other => {
                write!(out, "{now:7.3}  {gap:6.3}   {other:?}\r\n")?;
                out.flush()?;
            }
        }
    }
}
