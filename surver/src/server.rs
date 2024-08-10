//! Handling of external communication in Surver.
use bincode::Options;
use color_eyre::eyre::{anyhow, bail, Context};
use color_eyre::Result;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use log::{error, info, warn};
use rand::distributions::{Alphanumeric, DistString};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use wellen::{viewers, FileFormat, Hierarchy, Signal, SignalRef, Time};

use crate::{
    Status, BINCODE_OPTIONS, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, SURFER_VERSION,
    WELLEN_SURFER_DEFAULT_OPTIONS, WELLEN_VERSION, X_SURFER_VERSION, X_WELLEN_VERSION,
};

struct FileInfo {
    filename: String,
    hierarchy: Hierarchy,
    file_format: FileFormat,
    header_len: u64,
    body_len: u64,
    body_progress: Arc<AtomicU64>,
}
struct ReadOnly {
    url: String,
    token: String,
    files: Vec<Arc<FileInfo>>,
}

#[derive(Default)]
struct State {
    timetable: Vec<Time>,
    signals: HashMap<SignalRef, Signal>,
}

type SignalRequest = Vec<SignalRef>;

fn get_info_page(shared: &Arc<ReadOnly>) -> String {
    let head = format!(
        r#"
    <!DOCTYPE html><html lang="en">
    <head><title>Surver - Surfer Remote Server</title></head><body>
    <h1>Surver - Surfer Remote Server</h1>
    <b>To connect, run:</b> <code>surfer {}</code><br>
    <b>Wellen version:</b> {WELLEN_VERSION}<br>
    <b>Surfer version:</b> {SURFER_VERSION}<br>
    "#,
        shared.url
    );
    let mut strings = Vec::new();
    for file in &shared.files {
        let bytes_loaded: u64 = file.body_progress.load(Ordering::SeqCst);

        let progress = if bytes_loaded == file.body_len {
            format!(
                "{} loaded",
                bytesize::ByteSize::b(file.body_len + file.header_len)
            )
        } else {
            format!(
                "{} / {}",
                bytesize::ByteSize::b(bytes_loaded + file.header_len),
                bytesize::ByteSize::b(file.body_len + file.header_len)
            )
        };
        let res = format!(
            r#"
    <b>Filename:</b> {}<br>
    <b>Progress:</b> {progress}<br>
            "#,
            file.filename
        );
        strings.push(res);
    }
    format!("{head} {} </body></html>", strings.join("\n"))
}

fn get_hierarchy(shared: &Arc<FileInfo>) -> Result<Vec<u8>> {
    let mut raw = BINCODE_OPTIONS.serialize(&shared.file_format)?;
    let mut raw2 = BINCODE_OPTIONS.serialize(&shared.hierarchy)?;
    raw.append(&mut raw2);
    let compressed = lz4_flex::compress_prepend_size(&raw);
    info!(
        "Sending hierarchy. {} raw, {} compressed.",
        bytesize::ByteSize::b(raw.len() as u64),
        bytesize::ByteSize::b(compressed.len() as u64)
    );
    Ok(compressed)
}

fn get_file_list(shared: &Arc<ReadOnly>) -> Result<Vec<u8>> {
    let file_list = shared
        .files
        .clone()
        .into_iter()
        .map(|fi| fi.filename.clone())
        .collect::<Vec<String>>();
    let raw = BINCODE_OPTIONS.serialize(&file_list)?;
    let compressed = lz4_flex::compress_prepend_size(&raw);
    info!(
        "Sending file list. {} raw, {} compressed.",
        bytesize::ByteSize::b(raw.len() as u64),
        bytesize::ByteSize::b(compressed.len() as u64)
    );
    Ok(compressed)
}

async fn get_timetable(state: &Arc<RwLock<State>>) -> Result<Vec<u8>> {
    // poll to see when the time table is available
    #[allow(unused_assignments)]
    let mut raw = vec![];
    loop {
        {
            let state = state.read().unwrap();
            if !state.timetable.is_empty() {
                raw = BINCODE_OPTIONS.serialize(&state.timetable)?;
                break;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    let compressed = lz4_flex::compress_prepend_size(&raw);
    info!(
        "Sending timetable. {} raw, {} compressed.",
        bytesize::ByteSize::b(raw.len() as u64),
        bytesize::ByteSize::b(compressed.len() as u64)
    );
    Ok(compressed)
}

fn get_status(shared: &Arc<FileInfo>) -> Result<Vec<u8>> {
    let status = Status {
        bytes: shared.body_len + shared.header_len,
        bytes_loaded: shared.body_progress.load(Ordering::SeqCst) + shared.header_len,
        filename: shared.filename.clone(),
        wellen_version: WELLEN_VERSION.to_string(),
        surfer_version: SURFER_VERSION.to_string(),
        file_format: shared.file_format,
    };
    Ok(serde_json::to_vec(&status)?)
}

async fn get_signals(
    state: &Arc<RwLock<State>>,
    tx: &Sender<SignalRequest>,
    id_strings: &[&str],
) -> Result<Vec<u8>> {
    let mut ids = Vec::with_capacity(id_strings.len());
    for id in id_strings.iter() {
        ids.push(SignalRef::from_index(id.parse::<u64>()? as usize).unwrap());
    }

    if ids.is_empty() {
        return Ok(vec![]);
    }
    let num_ids = ids.len();

    // send request to background thread
    tx.send(ids.clone())?;

    // poll to see when all our ids are returned
    let mut raw = BINCODE_OPTIONS.serialize(&(num_ids as u64))?;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        {
            let state = state.read().unwrap();
            if ids.iter().all(|id| state.signals.contains_key(id)) {
                for id in ids {
                    raw.append(&mut BINCODE_OPTIONS.serialize(&id)?);
                    raw.append(&mut BINCODE_OPTIONS.serialize(&state.signals[&id])?);
                }
                break;
            }
        };
    }
    let compressed = lz4_flex::compress_prepend_size(&raw);
    info!(
        "Sending {} signals. {} raw, {} compressed.",
        num_ids,
        bytesize::ByteSize::b(raw.len() as u64),
        bytesize::ByteSize::b(compressed.len() as u64)
    );
    Ok(compressed)
}

const CONTENT_TYPE: &str = "Content-Type";
const JSON_MIME: &str = "application/json";

trait DefaultHeader {
    fn default_header(self) -> Self;
}

impl DefaultHeader for hyper::http::response::Builder {
    fn default_header(self) -> Self {
        self.header(HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER)
            .header(X_WELLEN_VERSION, WELLEN_VERSION)
            .header(X_SURFER_VERSION, SURFER_VERSION)
            .header("Cache-Control", "no-cache")
    }
}

async fn handle_cmd(
    state: &Arc<RwLock<State>>,
    shared: &Arc<FileInfo>,
    tx: &Sender<SignalRequest>,
    cmd: &str,
    args: &[&str],
) -> Result<Response<Full<Bytes>>> {
    let response = match (cmd, args) {
        ("get_status", []) => {
            let body = get_status(shared)?;
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, JSON_MIME)
                .default_header()
                .body(Full::from(body))
        }
        ("get_hierarchy", []) => {
            let body = get_hierarchy(shared)?;
            Response::builder()
                .status(StatusCode::OK)
                .default_header()
                .body(Full::from(body))
        }
        ("get_time_table", []) => {
            let body = get_timetable(state).await?;
            Response::builder()
                .status(StatusCode::OK)
                .default_header()
                .body(Full::from(body))
        }
        ("get_signals", id_strings) => {
            let body = get_signals(state, tx, id_strings).await?;
            Response::builder()
                .status(StatusCode::OK)
                .default_header()
                .body(Full::from(body))
        }
        _ => {
            // unknown command or unexpected number of arguments
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::from(vec![]))
        }
    };
    Ok(response?)
}

async fn handle(
    states: &Vec<Arc<RwLock<State>>>,
    shared: &Arc<ReadOnly>,
    txs: &Vec<Sender<SignalRequest>>,
    req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>> {
    // check to see if the correct token was received
    let path_parts = req.uri().path().split('/').skip(1).collect::<Vec<_>>();

    // check token
    if let Some(provided_token) = path_parts.first() {
        if *provided_token != shared.token {
            warn!(
                "Received request with invalid token: {provided_token} != {}\n{:?}",
                shared.token,
                req.uri()
            );
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::from(vec![]))?);
        }
    } else {
        // no token
        warn!("Received request with no token: {:?}", req.uri());
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::from(vec![]))?);
    }

    if let Some(idx_str) = path_parts.get(1) {
        if *idx_str == "get_file_list" {
            let body = get_file_list(shared)?;
            let response = Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, JSON_MIME)
                .default_header()
                .body(Full::from(body))?;
            Ok(response)
        } else {
            let idx = usize::from_str_radix(*idx_str, 10)?;
            // check command
            let response = if let Some(cmd) = path_parts.get(2) {
                handle_cmd(
                    states.get(idx).unwrap(),
                    shared.files.get(idx).unwrap(),
                    txs.get(idx).unwrap(),
                    cmd,
                    &path_parts[3..],
                )
                .await?
            } else {
                // valid token, but no command => return info
                let body = Full::from(get_info_page(shared));
                Response::builder()
                    .status(StatusCode::OK)
                    .default_header()
                    .body(body)?
            };
            Ok(response)
        }
    } else {
        // valid token, but no command => return info
        let body = Full::from(get_info_page(shared));
        Ok(Response::builder()
            .status(StatusCode::OK)
            .default_header()
            .body(body)?)
    }
}

const MIN_TOKEN_LEN: usize = 8;
const RAND_TOKEN_LEN: usize = 24;

pub type ServerStartedFlag = Arc<std::sync::atomic::AtomicBool>;

pub async fn server_main(
    port: u16,
    token: Option<String>,
    filenames: Vec<String>,
    started: Option<ServerStartedFlag>,
) -> Result<()> {
    // if no token was provided, we generate one
    let token = token.unwrap_or_else(|| {
        // generate a random ASCII token
        Alphanumeric.sample_string(&mut rand::thread_rng(), RAND_TOKEN_LEN)
    });

    if token.len() < MIN_TOKEN_LEN {
        bail!("Token `{token}` is too short. At least {MIN_TOKEN_LEN} characters are required!");
    }

    if filenames.len() < 1 {
        bail!("At least one file must be provided!");
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // immutable read-only data
    let url = format!("http://{addr:?}/{}", token);

    let mut states = Vec::new();
    let mut file_infos = Vec::new();
    let mut txs = Vec::new();
    for filename in filenames {
        // load file
        let start_read_header = web_time::Instant::now();
        let header_result =
            wellen::viewers::read_header(filename.as_str(), &WELLEN_SURFER_DEFAULT_OPTIONS)
                .map_err(|e| anyhow!("{e:?}"))
                .with_context(|| format!("Failed to parse wave file: {filename}"))?;
        info!(
            "Loaded header of {filename} in {:?}",
            start_read_header.elapsed()
        );
        let file_info = Arc::new(FileInfo {
            filename,
            hierarchy: header_result.hierarchy,
            file_format: header_result.file_format,
            header_len: 0, // FIXME: get value from wellen
            body_len: header_result.body_len,
            body_progress: Arc::new(AtomicU64::new(0)),
        });
        // channel to communicate with loader
        let (tx, rx) = std::sync::mpsc::channel::<SignalRequest>();

        // state can be written by the loading thread
        let state = Arc::new(RwLock::new(State::default()));
        // start work thread
        let state_2 = state.clone();
        let cloned_file_info = file_info.clone();
        std::thread::spawn(move || loader(&cloned_file_info, header_result.body, state_2, rx));
        states.push(state);
        file_infos.push(file_info);
        txs.push(tx);
    }

    let shared = Arc::new(ReadOnly {
        url: url.clone(),
        token: token.clone(),
        files: file_infos,
    });
    // print out status
    info!("Starting server on {addr:?}. To use:");
    info!("1. Setup an ssh tunnel: -L {port}:localhost:{port}");
    let hostname = whoami::fallible::hostname();
    if let Ok(hostname) = hostname.as_ref() {
        let username = whoami::username();
        info!(
            "   The correct command may be: ssh -L {port}:localhost:{port} {username}@{hostname} "
        );
    }

    info!("2. Start Surfer: surfer {url} ");
    if let Ok(hostname) = hostname {
        let hosturl = format!("http://{hostname}:{port}/{token}");
        info!("or, if the host is directly accessible:");
        info!("1. Start Surfer: surfer {hosturl} ");
    }
    // create listener and serve it
    let listener = TcpListener::bind(&addr).await?;

    // we have started the server
    if let Some(started) = started {
        started.store(true, Ordering::SeqCst);
    }

    // main server loop
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        let cloned_states = states.clone();
        let cloned_shared = shared.clone();
        let cloned_txs = txs.clone();

        tokio::task::spawn(async move {
            let service =
                service_fn(|req| handle(&cloned_states, &cloned_shared, &cloned_txs, req));
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                error!("server error: {}", e);
            }
        });
    }
}

/// Thread that loads the body and signals.
fn loader(
    shared: &Arc<FileInfo>,
    body_cont: viewers::ReadBodyContinuation,
    state: Arc<RwLock<State>>,
    rx: std::sync::mpsc::Receiver<SignalRequest>,
) -> Result<()> {
    // load the body of the file
    let start_load_body = web_time::Instant::now();
    let body_result = viewers::read_body(
        body_cont,
        &shared.hierarchy,
        Some(shared.body_progress.clone()),
    )
    .map_err(|e| anyhow!("{e:?}"))
    .with_context(|| format!("Failed to parse body of wave file: {}", shared.filename))?;
    info!(
        "Loaded body of {} in {:?}",
        shared.filename,
        start_load_body.elapsed()
    );

    // update state with body results
    {
        let mut state = state.write().unwrap();
        state.timetable = body_result.time_table;
    }
    // source is private, only owned by us
    let mut source = body_result.source;

    // process requests for signals to be loaded
    loop {
        let ids = rx.recv()?;

        // make sure that we do not load signals that have already been loaded
        let mut filtered_ids = {
            let state_lock = state.read().unwrap();
            ids.iter()
                .filter(|id| !state_lock.signals.contains_key(id))
                .cloned()
                .collect::<Vec<_>>()
        };

        // check if there is anything left to do
        if filtered_ids.is_empty() {
            continue;
        }

        // load signals without holding the lock
        filtered_ids.sort();
        filtered_ids.dedup();
        let result = source.load_signals(&filtered_ids, &shared.hierarchy, true);

        // store signals
        {
            let mut state = state.write().unwrap();
            for (id, signal) in result {
                state.signals.insert(id, signal);
            }
        }
    }

    // unreachable!("the user needs to terminate the server")
}
