//! Computer client and native in-runtime activation helper. Private operations read
//! typed stdin; command lines never carry signed storage URLs or initial admission.
#[path = "environment/build.rs"]
mod build;
#[path = "environment/client.rs"]
mod client;
#[path = "hub/activation/types.rs"]
mod definition;
#[path = "environment/lore.rs"]
mod lore;
#[path = "environment/materialize.rs"]
mod materialize;
#[path = "native_source_ids.rs"]
mod native_source_ids;
#[path = "environment/process.rs"]
mod process;
#[path = "environment/protocol.rs"]
mod protocol;
#[path = "environment/worker.rs"]
mod worker;
use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "veoveo-environment")]
struct Args {
    #[arg(long, default_value = "/sandbox", global = true)]
    root: PathBuf,
    #[command(subcommand)]
    command: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Apply(client::ApplyArgs),
    Watch(client::ApplyArgs),
    FollowDefinition(client::FollowArgs),
    Diagnostics,
    Materialize,
    MaterializeLore,
    Build,
    Describe,
    Register,
    Resume,
    Start,
    WorkerExec {
        #[arg(long)]
        build: veoveo_types::BuildId,
    },
}
fn stdin<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "private operation input exceeds bound"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn output(value: &impl serde::Serialize) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse();
    match args.command {
        Operation::Apply(connection) => client::run(connection, false).await,
        Operation::Watch(connection) => client::run(connection, true).await,
        Operation::FollowDefinition(connection) => client::follow(connection).await,
        Operation::Diagnostics => {
            let build: veoveo_types::BuildId = stdin()?;
            let path = args.root.join(format!("diagnostics/{build}.json"));
            let metadata = std::fs::metadata(&path)?;
            ensure!(metadata.len() <= 256 * 1024, "diagnostic exceeds bound");
            let result: protocol::BuildResult = serde_json::from_slice(&std::fs::read(path)?)?;
            output(&result)
        }
        Operation::MaterializeLore => output(&lore::run(&args.root, stdin()?).await?),
        Operation::Materialize => output(&materialize::run(&args.root, stdin()?).await?),
        Operation::Build => output(&build::run(&args.root, stdin()?).await?),
        Operation::Describe => output(&worker::describe(&args.root, &stdin()?).await?),
        Operation::Resume => output(&worker::resume(&args.root, &stdin()?).await?),
        Operation::Register => {
            worker::register(&args.root, stdin()?).await?;
            output(&())
        }
        Operation::Start => {
            worker::start(&args.root, &stdin()?)?;
            output(&())
        }
        Operation::WorkerExec { build } => worker::exec(&args.root, build),
    }
}
