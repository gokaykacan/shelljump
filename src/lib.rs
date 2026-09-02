//! ShellJump engine.
//!
//! The simulation half of the crate — [`math`], [`time`], [`world`],
//! [`entities`], [`physics`], [`collision`], [`camera`], [`game`], the root of
//! [`input`] and the root of [`render`] — is free of terminal dependencies and
//! testable headlessly. Crossterm is confined to three boundary modules:
//! [`terminal`], [`input::terminal`] and [`render::terminal`].

pub mod camera;
pub mod collision;
pub mod entities;
pub mod game;
pub mod input;
pub mod math;
pub mod physics;
pub mod render;
pub mod terminal;
pub mod time;
pub mod world;
