//! Crossterm event pump. The only place physical keys are known.

use std::io;
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::{GameKey, HoldMode, InputCollector, InputState};

fn map_key(code: KeyCode) -> Option<GameKey> {
    match code {
        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => Some(GameKey::Left),
        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => Some(GameKey::Right),
        KeyCode::Char(' ') => Some(GameKey::Jump),
        // Run is a letter key, not Shift. A bare modifier press/release reaches
        // us only on terminals running the Kitty protocol; everywhere else it
        // produces no event at all, so a Shift binding would silently do
        // nothing on exactly the terminals that need Run most.
        KeyCode::Char('j') | KeyCode::Char('J') => Some(GameKey::Run),
        _ => None,
    }
}

fn is_quit(key: &KeyEvent) -> bool {
    // Raw mode suppresses the terminal's own SIGINT translation, so Ctrl+C
    // arrives here as an ordinary key event and must be handled explicitly.
    let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
    ctrl_c
        || matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q')
        )
}

pub struct EventPump {
    collector: InputCollector,
}

impl EventPump {
    pub fn new(mode: HoldMode) -> Self {
        Self {
            collector: InputCollector::new(mode),
        }
    }

    /// Consumes terminal events until `deadline`, then returns. Blocks only for
    /// the slack left in the frame, so this doubles as the frame-rate limiter
    /// while keeping input latency at the event's own arrival time.
    pub fn pump_until(&mut self, deadline: Instant, epoch: Instant) -> io::Result<()> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !event::poll(remaining)? {
                return Ok(());
            }
            let now = epoch.elapsed().as_secs_f64();
            self.handle(event::read()?, now);
        }
    }

    fn handle(&mut self, ev: Event, now: f64) {
        match ev {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    if let Some(game_key) = map_key(key.code) {
                        self.collector.on_release(game_key);
                    }
                    return;
                }
                if is_quit(&key) {
                    self.collector.request_quit();
                } else if let Some(game_key) = map_key(key.code) {
                    self.collector.on_press(game_key, now);
                }
            }
            Event::Resize(columns, rows) => self.collector.on_resize(columns, rows),
            _ => {}
        }
    }

    pub fn finish_frame(&mut self, now: f64) -> InputState {
        self.collector.finish_frame(now)
    }

    pub fn acknowledge_edges(&mut self) {
        self.collector.acknowledge_edges();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn movement_and_jump_keys_map_to_actions() {
        assert_eq!(map_key(KeyCode::Char('a')), Some(GameKey::Left));
        assert_eq!(map_key(KeyCode::Left), Some(GameKey::Left));
        assert_eq!(map_key(KeyCode::Char('D')), Some(GameKey::Right));
        assert_eq!(map_key(KeyCode::Char(' ')), Some(GameKey::Jump));
        assert_eq!(map_key(KeyCode::Char('j')), Some(GameKey::Run));
        assert_eq!(map_key(KeyCode::Char('J')), Some(GameKey::Run));
        assert_eq!(map_key(KeyCode::Char('z')), None);
    }

    #[test]
    fn the_run_key_does_not_collide_with_quit_or_movement() {
        for code in [
            KeyCode::Char('a'),
            KeyCode::Char('d'),
            KeyCode::Char(' '),
            KeyCode::Left,
            KeyCode::Right,
        ] {
            assert_ne!(map_key(code), Some(GameKey::Run));
        }
        assert!(!is_quit(&KeyEvent::from(KeyCode::Char('j'))));
        assert!(!is_quit(&KeyEvent::from(KeyCode::Char('J'))));
    }

    #[test]
    fn quit_keys_include_ctrl_c() {
        let ctrl_c = KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        assert!(is_quit(&ctrl_c));
        assert!(is_quit(&KeyEvent::from(KeyCode::Esc)));
        assert!(is_quit(&KeyEvent::from(KeyCode::Char('q'))));
        assert!(!is_quit(&KeyEvent::from(KeyCode::Char('c'))));
    }

    #[test]
    fn a_batch_of_events_folds_into_one_input_state() {
        let mut pump = EventPump::new(HoldMode::Explicit);
        pump.handle(press(KeyCode::Char('d')), 0.0);
        pump.handle(press(KeyCode::Char(' ')), 0.0);
        pump.handle(press(KeyCode::Char('j')), 0.0);
        pump.handle(Event::Resize(100, 30), 0.0);

        let state = pump.finish_frame(0.0);
        assert!(state.move_right);
        assert!(state.run_held);
        assert!(state.jump_held);
        assert!(state.jump_pressed);
        assert!(!state.quit_requested);
        assert_eq!(state.resized, Some((100, 30)));
    }

    #[test]
    fn escape_requests_quit() {
        let mut pump = EventPump::new(HoldMode::Explicit);
        pump.handle(press(KeyCode::Esc), 0.0);
        assert!(pump.finish_frame(0.0).quit_requested);
    }
}
