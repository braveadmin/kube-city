//! The main loop: apply incoming events, draw a frame, handle keys.

use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};
use tokio::sync::{mpsc::UnboundedReceiver, watch};

use crate::{
    render::{self, Camera},
    world::{World, WorldEvent},
};

/// Pixels moved per key press; horizontal steps are wider to match tiles.
const PAN_X: i32 = 8;
const PAN_Y: i32 = 4;

pub fn run(
    terminal: &mut DefaultTerminal,
    events: &mut UnboundedReceiver<WorldEvent>,
    fps: u32,
    source: &watch::Receiver<String>,
) -> Result<()> {
    let frame_time = Duration::from_secs(1) / fps;
    let mut world = World::default();
    let mut camera = Camera::default();
    loop {
        while let Ok(event) = events.try_recv() {
            world.apply(event);
        }
        let source = source.borrow().clone();
        terminal.draw(|frame| render::draw(frame, &world, camera, &source))?;

        let deadline = Instant::now() + frame_time;
        while let Some(timeout) = deadline.checked_duration_since(Instant::now()) {
            if !event::poll(timeout)? {
                break;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Left | KeyCode::Char('h') => camera.pan(PAN_X, 0),
                KeyCode::Right | KeyCode::Char('l') => camera.pan(-PAN_X, 0),
                KeyCode::Up | KeyCode::Char('k') => camera.pan(0, PAN_Y),
                KeyCode::Down | KeyCode::Char('j') => camera.pan(0, -PAN_Y),
                KeyCode::Char('c') => camera = Camera::default(),
                _ => {}
            }
        }
    }
}
