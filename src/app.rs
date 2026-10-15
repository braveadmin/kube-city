//! The main loop: apply incoming events, draw a frame, handle keys.

use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};
use tokio::sync::{mpsc::UnboundedReceiver, watch};

use crate::{
    render::{self, View, theme::Theme},
    world::{World, WorldEvent},
};

/// Pixels moved per key press; horizontal steps are wider to match tiles.
const PAN_X: i32 = 8;
const PAN_Y: i32 = 4;
/// One animation step: fire, smoke and cranes move at this pace.
const ANIMATION_STEP: Duration = Duration::from_millis(100);

pub fn run(
    terminal: &mut DefaultTerminal,
    events: &mut UnboundedReceiver<WorldEvent>,
    fps: u32,
    source: &watch::Receiver<String>,
    theme: &dyn Theme,
) -> Result<()> {
    let frame_time = Duration::from_secs(1) / fps;
    let started = Instant::now();
    let mut world = World::default();
    let mut view = View::default();
    loop {
        while let Ok(event) = events.try_recv() {
            world.apply(event);
        }
        let source = source.borrow().clone();
        let anim = (started.elapsed().as_millis() / ANIMATION_STEP.as_millis()) as u64;
        terminal.draw(|frame| render::draw(frame, &world, view, theme, anim, &source))?;

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
                KeyCode::Left | KeyCode::Char('h') => view.pan(PAN_X, 0),
                KeyCode::Right | KeyCode::Char('l') => view.pan(-PAN_X, 0),
                KeyCode::Up | KeyCode::Char('k') => view.pan(0, PAN_Y),
                KeyCode::Down | KeyCode::Char('j') => view.pan(0, -PAN_Y),
                KeyCode::Char('z') => view.toggle_zoom(),
                KeyCode::Char('c') => view.recentre(),
                _ => {}
            }
        }
    }
}
