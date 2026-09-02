//! Crossterm framebuffer writer. Emits only the spans that changed.

use std::io::{self, BufWriter, Stdout, Write};

use crossterm::style::{Color, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{QueueableCommand, cursor, queue, style};

use super::{DrawRun, Framebuffer, Rgb, compute_runs};

fn to_crossterm(color: Rgb) -> Color {
    Color::Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

pub struct TerminalRenderer {
    out: BufWriter<Stdout>,
    previous: Framebuffer,
    runs: Vec<DrawRun>,
    text: String,
    full_redraw: bool,
    last_colors: Option<(Rgb, Rgb)>,
}

impl TerminalRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            // One buffered writer for the whole session: the frame is queued in
            // full and flushed exactly once.
            out: BufWriter::with_capacity(256 * 1024, io::stdout()),
            previous: Framebuffer::new(width, height),
            runs: Vec::new(),
            text: String::new(),
            full_redraw: true,
            last_colors: None,
        }
    }

    /// Invalidates the cached frame so the next present repaints everything.
    pub fn invalidate(&mut self) {
        self.full_redraw = true;
    }

    pub fn present(&mut self, frame: &Framebuffer) -> io::Result<()> {
        let previous = if self.full_redraw {
            None
        } else {
            Some(&self.previous)
        };
        compute_runs(frame, previous, &mut self.runs);

        if self.runs.is_empty() {
            return Ok(());
        }

        if self.full_redraw {
            self.last_colors = None;
            queue!(self.out, Clear(ClearType::All))?;
        }

        for run in &self.runs {
            self.text.clear();
            for offset in 0..run.len {
                self.text.push(frame.cell(run.x + offset, run.y).glyph);
            }
            self.out.queue(cursor::MoveTo(run.x, run.y))?;
            if self.last_colors != Some((run.fg, run.bg)) {
                self.out.queue(SetForegroundColor(to_crossterm(run.fg)))?;
                self.out.queue(SetBackgroundColor(to_crossterm(run.bg)))?;
                self.last_colors = Some((run.fg, run.bg));
            }
            self.out.write_all(self.text.as_bytes())?;
        }

        self.out.flush()?;
        self.previous.copy_from(frame);
        self.full_redraw = false;
        Ok(())
    }

    /// Restores default colours. Called before leaving the alternate screen so
    /// the host shell is not left with our palette applied.
    pub fn reset_colors(&mut self) -> io::Result<()> {
        queue!(self.out, style::ResetColor)?;
        self.out.flush()
    }
}
