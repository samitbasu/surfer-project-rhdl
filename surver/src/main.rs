//! Code for the `surver` executable.
use std::fs::File;
use std::io::{BufRead, BufReader};

use clap::Parser;
use color_eyre::Result;
use fern::colors::ColoredLevelConfig;
use fern::Dispatch;

#[derive(Parser)]
#[command(version, about, arg_required_else_help = true)]
struct Arguments {
    #[clap(flatten)]
    file_group: FileGroup,
    /// Port on which server will listen
    #[clap(long)]
    port: Option<u16>,
    /// Token used by the client to authenticate to the server
    #[clap(long)]
    token: Option<String>,
}

#[derive(Debug, Default, clap::Args)]
#[group(required = true)]
pub struct FileGroup {
    /// Waveform files in VCD, FST, or GHW format.
    wave_files: Vec<String>,
    /// File with one wave form file name per line
    #[clap(long)]
    file: Option<String>,
}

/// Starts the logging and error handling. Can be used by unittests to get more insights.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_logging() -> Result<()> {
    let colors = ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Green)
        .debug(fern::colors::Color::Blue)
        .trace(fern::colors::Color::White);

    let stdout_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surver", log::LevelFilter::Trace)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message
            ));
        })
        .chain(std::io::stdout());

    Dispatch::new().chain(stdout_config).apply()?;

    color_eyre::install()?;

    Ok(())
}

fn main() -> Result<()> {
    start_logging()?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    // parse arguments
    let args = Arguments::parse();
    let default_port = 8911; // FIXME: make this more configurable

    // Handle file lists
    let mut file_names = args.file_group.wave_files.clone();

    // Append file names from file
    if let Some(filename) = args.file_group.file {
        let file = File::open(filename).expect("no such file");
        let buf = BufReader::new(file);
        let mut files = buf
            .lines()
            .map(|l| l.expect("Could not parse line"))
            .filter(|s| !s.is_empty())
            .collect::<Vec<String>>();
        file_names.append(&mut files);
    }

    runtime.block_on(surver::server_main(
        args.port.unwrap_or(default_port),
        args.token,
        file_names,
        None,
    ))
}
