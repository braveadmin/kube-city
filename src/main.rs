//! kube-city: a living isometric city of your Kubernetes cluster.

mod app;
mod cluster;
mod demo;
mod layout;
mod recording;
mod render;
mod util;
mod world;

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use clap::Parser;
use tokio::sync::{mpsc, watch};

#[derive(Debug, Parser)]
#[command(version, about = "A living isometric city of your Kubernetes cluster")]
struct Args {
    /// Show a simulated cluster instead of a real one.
    #[arg(long, conflicts_with_all = ["context", "namespaces", "replay"])]
    demo: bool,

    /// Kubeconfig context to show (default: the current context).
    #[arg(long)]
    context: Option<String>,

    /// Only show this namespace; repeat for more (default: all namespaces).
    #[arg(long = "namespace", short = 'n', value_name = "NAMESPACE")]
    namespaces: Vec<String>,

    /// Show a city recorded earlier with --record instead of a live cluster.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["context", "namespaces", "record"])]
    replay: Option<PathBuf>,

    /// Replay speed: 2 plays twice as fast as recorded.
    #[arg(long, default_value_t = 1.0)]
    replay_speed: f64,

    /// Also write everything that happens to FILE, for --replay later.
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,

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
    ensure!((0.1..=100.0).contains(&args.demo_rate), "--demo-rate must be between 0.1 and 100");
    ensure!((0.01..=1000.0).contains(&args.replay_speed), "--replay-speed must be between 0.01 and 1000");

    let runtime = tokio::runtime::Runtime::new()?;
    let (sender, from_source) = mpsc::unbounded_channel();
    let source = if args.demo {
        let seed = args.seed.unwrap_or_else(seed_from_clock);
        runtime.spawn(demo::run(seed, args.demo_rate, sender));
        watch::channel(format!("demo seed {seed}")).1
    } else if let Some(path) = &args.replay {
        let recording = recording::load(path)?;
        runtime.spawn(recording::replay(recording, args.replay_speed, sender));
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        watch::channel(format!("replay {name}")).1
    } else {
        // Connect before taking over the terminal, so login prompts and
        // errors show up as plain text.
        let (client, context) = runtime.block_on(cluster::connect(args.context.as_deref(), &args.namespaces))?;
        let (status, source) = watch::channel(format!("{context} · connecting"));
        runtime.spawn(cluster::watch(client, context, args.namespaces, sender, status));
        source
    };

    let mut receiver = match &args.record {
        None => from_source,
        Some(path) => {
            let file = std::fs::File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
            let (to_city, receiver) = mpsc::unbounded_channel();
            runtime.spawn(recording::tee(from_source, to_city, tokio::fs::File::from_std(file)));
            receiver
        }
    };

    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, &mut receiver, args.fps, &source, &render::theme::Metropolis);
    ratatui::restore();
    result
}

fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |elapsed| elapsed.as_nanos() as u64)
}
