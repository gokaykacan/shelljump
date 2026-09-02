//! QA-only harness: enters the real terminal guard exactly like `main`, then
//! panics partway through a fake frame loop. Used by an external test driver
//! (attached to a pty) to prove the terminal is restored even when the panic
//! happens mid-loop, not just on a clean quit path.
//!
//! Not part of the shipped binary; does not change production behavior.

use shelljump::terminal::TerminalGuard;

fn main() {
    shelljump::terminal::install_panic_hook();
    let _guard = TerminalGuard::enter().expect("enter terminal");

    // Simulate a few frames of work before the injected failure, so this
    // exercises "panic inside the loop" rather than "panic before setup".
    for i in 0..3 {
        if i == 2 {
            panic!("injected panic for terminal-restoration QA");
        }
    }
}
