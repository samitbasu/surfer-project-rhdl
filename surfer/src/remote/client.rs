use bincode::Options;
use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;
use log::info;

use super::HierarchyResponse;

use surver::{
    Status, BINCODE_OPTIONS, HTTP_SERVER_KEY, HTTP_SERVER_VALUE_SURFER, SURFER_VERSION,
    WELLEN_VERSION, X_SURFER_VERSION, X_WELLEN_VERSION,
};

fn check_response(server_url: &str, response: &reqwest::Response) -> Result<()> {
    let server = response
        .headers()
        .get(HTTP_SERVER_KEY)
        .ok_or(eyre!("no server header"))?
        .to_str()?;
    if server != HTTP_SERVER_VALUE_SURFER {
        bail!("Unexpected server {server} from {server_url}");
    }
    let surfer_version = response
        .headers()
        .get(X_SURFER_VERSION)
        .ok_or(eyre!("no surfer version header"))?
        .to_str()?;
    if surfer_version != SURFER_VERSION {
        // this mismatch may be OK as long as the wellen version matches
        info!("Surfer version on the server: {surfer_version} does not match client version {SURFER_VERSION}");
    }
    let wellen_version = response
        .headers()
        .get(X_WELLEN_VERSION)
        .ok_or(eyre!("no wellen version header"))?
        .to_str()?;
    if wellen_version != WELLEN_VERSION {
        bail!("Version incompatibility! The server uses wellen {wellen_version}, our client uses wellen {WELLEN_VERSION}");
    }
    Ok(())
}

pub async fn get_status(server: String) -> Result<Status> {
    let client = reqwest::Client::new();
    let response = client.get(format!("{server}/get_status")).send().await?;
    check_response(&server, &response)?;
    let body = response.text().await?;
    let status = serde_json::from_str::<Status>(&body)?;
    Ok(status)
}

pub async fn get_hierarchy(server: String) -> Result<HierarchyResponse> {
    let client = reqwest::Client::new();
    let response = client.get(format!("{server}/get_hierarchy")).send().await?;
    check_response(&server, &response)?;
    let compressed = response.bytes().await?;
    let raw = lz4_flex::decompress_size_prepended(&compressed)?;
    let mut reader = std::io::Cursor::new(raw);
    // first we read a value, expecting there to be more bytes
    let opts = BINCODE_OPTIONS.allow_trailing_bytes();
    let file_format: wellen::FileFormat = opts.deserialize_from(&mut reader)?;
    // the last value should consume all remaining bytes
    let hierarchy: wellen::Hierarchy = BINCODE_OPTIONS.deserialize_from(&mut reader)?;
    Ok(HierarchyResponse {
        hierarchy,
        file_format,
    })
}

pub async fn get_time_table(server: String) -> Result<Vec<wellen::Time>> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{server}/get_time_table"))
        .send()
        .await?;
    check_response(&server, &response)?;
    let compressed = response.bytes().await?;
    let raw = lz4_flex::decompress_size_prepended(&compressed)?;
    let table = BINCODE_OPTIONS.deserialize(&raw)?;
    Ok(table)
}

pub async fn get_signals(
    server: String,
    signals: &[wellen::SignalRef],
) -> Result<Vec<(wellen::SignalRef, wellen::Signal)>> {
    let client = reqwest::Client::new();
    let mut url = format!("{server}/get_signals");
    for signal in signals.iter() {
        url.push_str(&format!("/{}", signal.index()));
    }

    let response = client.get(url).send().await?;
    check_response(&server, &response)?;
    let compressed = response.bytes().await?;
    let raw = lz4_flex::decompress_size_prepended(&compressed)?;

    let mut reader = std::io::Cursor::new(raw);
    let opts = BINCODE_OPTIONS.allow_trailing_bytes();
    let num_ids: u64 = opts.deserialize_from(&mut reader)?;
    if num_ids > signals.len() as u64 {
        bail!(
            "Too many signals in response: {num_ids}, expected {}",
            signals.len()
        );
    }
    if num_ids == 0 {
        return Ok(vec![]);
    }

    let mut out = Vec::with_capacity(num_ids as usize);
    for _ in 0..(num_ids - 1) {
        let signal_id: wellen::SignalRef = opts.deserialize_from(&mut reader)?;
        let signal: wellen::Signal = opts.deserialize_from(&mut reader)?;
        out.push((signal_id, signal));
    }
    // for the final signal, we expect to consume all bytes
    let signal_id: wellen::SignalRef = opts.deserialize_from(&mut reader)?;
    let signal: wellen::Signal = BINCODE_OPTIONS.deserialize_from(&mut reader)?;
    out.push((signal_id, signal));
    Ok(out)
}
