//! kube-city: a living isometric city of your Kubernetes cluster.

mod app;
mod demo;
mod layout;
mod render;
mod util;
mod world;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail, ensure};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "A living isometric city of your Kubernetes cluster")]
struct Args {
    /// Show a simulated cluster instead of a real one.
    #[arg(long)]
    demo: bool,

    /// Demo simulation steps per second (0.1 to 100).
    #[arg(long, default_value_t = 3.0)]
    demo_rate: f64,

    /// Demo seed; the same seed replays the same city.
    #[arg(long)]
    seed: Option<u64>,

    /// Frames per second.
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=60))]
    fps: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if !args.demo {
        bail!("watching a real cluster is not implemented yet; run with --demo for now");
    }
    ensure!((0.1..=100.0).contains(&args.demo_rate), "--demo-rate must be between 0.1 and 100");

    let seed = args.seed.unwrap_or_else(seed_from_clock);
    let runtime = tokio::runtime::Runtime::new()?;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    runtime.spawn(demo::run(seed, args.demo_rate, sender));

    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, &mut receiver, args.fps, &format!("demo seed {seed}"));
    ratatui::restore();
    result
}

fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |elapsed| elapsed.as_nanos() as u64)
}
