use clap::Parser;
use color_eyre::Result;
use fern::colors::ColoredLevelConfig;
use fern::Dispatch;

#[derive(clap::Parser, Default)]
#[command(version, about)]
struct Args {
    /// Waveform file in VCD, FST, or GHW format.
    wave_file: String,
    /// Port on which server will listen
    #[clap(long)]
    port: Option<u16>,
    /// Token used by the client to authenticate to the server
    #[clap(long)]
    token: Option<String>,
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
            ))
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
    let args = Args::parse();
    let default_port = 8911; // FIXME: make this more configurable
    runtime.block_on(surver::server_main(
        args.port.unwrap_or(default_port),
        args.token,
        args.wave_file,
        None,
    ))
}
