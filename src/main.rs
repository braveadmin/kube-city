//! kube-city: a living isometric city of your Kubernetes cluster.

mod anonymise;
mod cluster;
mod demo;
mod recording;
mod util;
mod web;
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

    /// Port for the city's page; it is only served on 127.0.0.1.
    #[arg(long, default_value_t = 7700)]
    port: u16,

    /// Do not open a browser; just print the address of the city.
    #[arg(long)]
    no_open: bool,

    /// Show the city in a browser. This is the default; the flag is kept so
    /// existing commands keep working.
    #[arg(long)]
    web: bool,

    /// Removed in 0.4.0; kept so that it says so instead of "unexpected
    /// argument".
    #[arg(long, hide = true)]
    tui: bool,

    /// Replace every name (namespaces, workloads, pods, nodes, images,
    /// contexts) with a made-up one in the page and in --record, so the city
    /// can be shared. The names are new every run.
    #[arg(long)]
    anonymise: bool,

    /// With --anonymise: the same salt gives the same made-up names every
    /// run. Keep it secret, or names could be matched by guessing.
    #[arg(long, value_name = "SALT", requires = "anonymise")]
    anonymise_salt: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        !args.tui,
        "the terminal view (--tui) was removed in 0.4.0; `cargo install kube-city --version 0.3.0` still has it"
    );
    ensure!((0.1..=100.0).contains(&args.demo_rate), "--demo-rate must be between 0.1 and 100");
    ensure!((0.01..=1000.0).contains(&args.replay_speed), "--replay-speed must be between 0.01 and 1000");

    let runtime = tokio::runtime::Runtime::new()?;
    let _runtime = runtime.enter();
    let anonymiser = args.anonymise.then(|| anonymise::Anonymiser::shared(args.anonymise_salt.clone()));
    let (sender, from_source) = mpsc::unbounded_channel();
    // In the browser a page may switch to another context; not while
    // recording, which holds a single cluster.
    let (source, shows) = if args.demo {
        let seed = args.seed.unwrap_or_else(seed_from_clock);
        runtime.spawn(demo::run(seed, args.demo_rate, sender));
        (watch::channel(format!("demo seed {seed}")).1, web::Shows::Demo)
    } else if let Some(path) = &args.replay {
        let recording = recording::load(path)?;
        runtime.spawn(recording::replay(recording, args.replay_speed, sender));
        let name = if args.anonymise {
            // The file name may say whose cluster it is.
            "recording".to_owned()
        } else {
            path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
        };
        (watch::channel(format!("replay {name}")).1, web::Shows::Replay)
    } else {
        // Connect before anything else, so login prompts and errors show
        // up as plain text.
        let (client, context) = runtime.block_on(cluster::connect(args.context.as_deref(), &args.namespaces))?;
        let contexts = if args.record.is_none() { cluster::contexts()? } else { Vec::new() };
        let clusters = web::Clusters { contexts, current: context.clone(), namespaces: args.namespaces.clone() };
        let (status, source) = watch::channel(format!("{context} · connecting"));
        runtime.spawn(cluster::watch(client, context, args.namespaces.clone(), sender, status));
        (source, web::Shows::Cluster(clusters))
    };

    // Before recording, so that recordings hold no real names either.
    let (from_source, source) = match &anonymiser {
        Some(anonymiser) => anonymise::stage(anonymiser.clone(), from_source, source),
        None => (from_source, source),
    };

    let receiver = match &args.record {
        None => from_source,
        Some(path) => {
            let file = std::fs::File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
            let (to_city, receiver) = mpsc::unbounded_channel();
            runtime.spawn(recording::tee(from_source, to_city, tokio::fs::File::from_std(file)));
            receiver
        }
    };

    runtime.block_on(web::serve(receiver, source, args.port, !args.no_open, shows, anonymiser))
}

fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |elapsed| elapsed.as_nanos() as u64)
}
