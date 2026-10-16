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

    /// Evaluation: hide finished jobs and give each DaemonSet one building.
    #[arg(long)]
    prototype: bool,

    /// Experimental: draw real pixels with the terminal's graphics protocol
    /// (Sixel in Windows Terminal) instead of half blocks.
    #[cfg(feature = "sixel")]
    #[arg(long)]
    sixel: bool,

    /// With --sixel: how much larger than half blocks to draw. 3 keeps the
    /// pixel-art look; about 10 uses every real pixel and is much slower.
    #[cfg(feature = "sixel")]
    #[arg(long, default_value_t = 3.0, requires = "sixel")]
    pixel_scale: f32,

    /// With --sixel: the size of a character cell in pixels, like 10x20, when
    /// the terminal doesn't report it. Too small and the city is cut off at
    /// the right and bottom; too large and it doesn't fill the window.
    #[cfg(feature = "sixel")]
    #[arg(long, value_name = "WxH", value_parser = parse_cell_size, requires = "sixel")]
    cell_size: Option<(u16, u16)>,

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
    let rules = world::Rules { hide_completed: args.prototype, group_daemonsets: args.prototype };
    #[cfg(feature = "sixel")]
    let pixels = args.sixel.then_some((args.pixel_scale, args.cell_size));
    #[cfg(not(feature = "sixel"))]
    let pixels = None;
    let result = app::run(&mut terminal, &mut receiver, args.fps, &source, &render::theme::Metropolis, rules, pixels);
    ratatui::restore();
    result
}

#[cfg(feature = "sixel")]
fn parse_cell_size(text: &str) -> Result<(u16, u16), String> {
    let invalid = || format!("{text:?} is not a cell size like 10x20");
    let (width, height) = text.split_once(['x', 'X', '×']).ok_or_else(invalid)?;
    let (width, height) = (width.trim().parse().map_err(|_| invalid())?, height.trim().parse().map_err(|_| invalid())?);
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    Ok((width, height))
}

fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |elapsed| elapsed.as_nanos() as u64)
}

#[cfg(all(test, feature = "sixel"))]
mod tests {
    use super::parse_cell_size;

    #[test]
    fn cell_sizes_read_like_10x20() {
        assert_eq!(parse_cell_size("10x20"), Ok((10, 20)));
        assert_eq!(parse_cell_size("9×19"), Ok((9, 19)));
        assert!(parse_cell_size("10").is_err());
        assert!(parse_cell_size("0x20").is_err());
        assert!(parse_cell_size("tenxtwenty").is_err());
    }
}
