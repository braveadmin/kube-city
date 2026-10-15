//! kube-city: a living isometric city of your Kubernetes cluster.

mod app;
mod cluster;
mod demo;
mod layout;
mod render;
mod util;
mod world;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, ensure};
use clap::Parser;
use tokio::sync::watch;

#[derive(Debug, Parser)]
#[command(version, about = "A living isometric city of your Kubernetes cluster")]
struct Args {
    /// Show a simulated cluster instead of a real one.
    #[arg(long, conflicts_with_all = ["context", "namespaces"])]
    demo: bool,

    /// Kubeconfig context to show (default: the current context).
    #[arg(long)]
    context: Option<String>,

    /// Only show this namespace; repeat for more (default: all namespaces).
    #[arg(long = "namespace", short = 'n', value_name = "NAMESPACE")]
    namespaces: Vec<String>,

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

    let runtime = tokio::runtime::Runtime::new()?;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let source = if args.demo {
        let seed = args.seed.unwrap_or_else(seed_from_clock);
        runtime.spawn(demo::run(seed, args.demo_rate, sender));
        watch::channel(format!("demo seed {seed}")).1
    } else {
        // Connect before taking over the terminal, so login prompts and
        // errors show up as plain text.
        let (client, context) = runtime.block_on(cluster::connect(args.context.as_deref(), &args.namespaces))?;
        let (status, source) = watch::channel(format!("{context} · connecting"));
        runtime.spawn(cluster::watch(client, context, args.namespaces, sender, status));
        source
    };

    let mut terminal = ratatui::init();
    let result = app::run(&mut terminal, &mut receiver, args.fps, &source);
    ratatui::restore();
    result
}

fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |elapsed| elapsed.as_nanos() as u64)
}
