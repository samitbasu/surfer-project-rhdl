mod logs;

use clap::Parser;
use color_eyre::Result;
use fern::colors::ColoredLevelConfig;
use fern::Dispatch;

#[derive(clap::Parser, Default)]
#[command(version)]
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

fn setup_logging(platform_logger: Dispatch) -> Result<()> {
    let surver_log_config = Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surfer", log::LevelFilter::Trace)
        .format(move |out, message, _record| out.finish(format_args!(" {}", message)))
        .chain(&logs::SURVER_LOGGER as &(dyn log::Log + 'static));

    Dispatch::new()
        .chain(platform_logger)
        .chain(surver_log_config)
        .apply()?;
    Ok(())
}

/// Starts the logging and error handling. Can be used by unittests to get more insights.
#[cfg(not(target_arch = "wasm32"))]
#[inline]
pub fn start_logging() -> Result<()> {
    let colors = ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Green)
        .debug(fern::colors::Color::Blue)
        .trace(fern::colors::Color::White);

    let stdout_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .level_for("surfer", log::LevelFilter::Trace)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message
            ))
        })
        .chain(std::io::stdout());
    setup_logging(stdout_config)?;

    color_eyre::install()?;
    Ok(())
}

fn main() -> Result<()> {
    start_logging()?;

    // https://tokio.rs/tokio/topics/bridging
    // We want to run the gui in the main thread, but some long running tasks like
    // loading VCDs should be done asynchronously. We can't just use std::thread to
    // do that due to wasm support, so we'll start a tokio runtime
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
