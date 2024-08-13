use crate::message::Message;
use crate::tests::snapshot::render_and_compare;
use crate::wcp::wcp_handler::{WcpCommand, WcpMessage};
use crate::State;

use itertools::Itertools;
use num::BigInt;
use serde::Deserialize;
use serde_json::Error as serde_Error;

use lazy_static::lazy_static;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{io::Write, net::TcpStream, thread, time::Duration, vec};

fn get_test_port() -> usize {
    lazy_static! {
        static ref PORT_NUM: Arc<Mutex<usize>> = Arc::new(Mutex::new(54321));
    }
    let mut port = PORT_NUM.lock().unwrap();
    *port += 1;
    *port
}

fn get_json_message(mut stream: &TcpStream) -> Result<WcpMessage, serde_Error> {
    let mut de = serde_json::Deserializer::from_reader(&mut stream);
    WcpMessage::deserialize(&mut de)
}

async fn run_test_client(port: usize, msgs: Vec<WcpMessage>, test_done: Arc<AtomicBool>) {
    let mut client: TcpStream;
    loop {
        if let Ok(c) = TcpStream::connect(format!("127.0.0.1:{port}")) {
            client = c;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    // read greeting message
    let _ = get_json_message(&mut client).expect("Could not read greeting message");
    // TODO check response content
    // clear screen
    let _ = serde_json::to_writer(&client, &WcpMessage::Command(WcpCommand::Clear));
    let _ = client.write(b"\0");
    let _ = client.flush();
    for message in msgs.into_iter() {
        // send message to Surfer
        let _ = serde_json::to_writer(&client, &message);
        let _ = client.write(b"\0");
        let _ = client.flush();
        // read response from Surfer
        let _ = get_json_message(&mut client);
        // sleep so that signals get sent in correct order
        thread::sleep(Duration::from_millis(100));
    }

    let _ = client.shutdown(std::net::Shutdown::Both);
    test_done.store(true, Ordering::SeqCst);
}

fn start_test_client(port: usize, msgs: Vec<WcpMessage>, test_done: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let _res = runtime.block_on(run_test_client(port, msgs, test_done));
    });
}

fn run_with_wcp(port: usize, test_done: Arc<AtomicBool>) -> State {
    // create state and add messages as batch commands
    let mut state = State::new_default_config().unwrap();

    let setup_msgs = vec![
        // hide GUI elements
        Message::StartWcpServer(Some(format!("127.0.0.1:{port}").to_string())),
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
    ];

    for msg in setup_msgs {
        state.update(msg);
    }

    // update state until all batch commands have been processed
    while !test_done.load(Ordering::SeqCst) {
        state.handle_async_messages();
        state.handle_wcp_commands();
    }

    state.stop_wcp_server();

    state
}

macro_rules! wcp_snapshot_with_commands {
    ($name:ident, $msgs:expr) => {
        #[test]
        fn $name() {
            let msgs: Vec<WcpMessage> = $msgs
                .into_iter()
                .map(|message| WcpMessage::Command(message))
                .collect();

            let port = get_test_port();
            let test_done = Arc::new(AtomicBool::new(false));
            let test_done_copy = test_done.clone();
            let test_client_thread =
                thread::spawn(move || start_test_client(port, msgs, test_done));
            let mut test_name = "wcp/".to_string();
            test_name.push_str(stringify!($name));

            render_and_compare(&PathBuf::from(&test_name), || {
                run_with_wcp(port, test_done_copy.clone())
            });

            let _ = test_client_thread.join();
        }
    };
}

wcp_snapshot_with_commands! {add_variables, vec![
    WcpCommand::Load{source:  "../examples/counter.vcd".to_string()},
    WcpCommand::AddVariables{names: vec![
        "tb._tmp",
        "tb.clk",
        "tb.overflow",
        "tb.reset"].into_iter().map(str::to_string).collect_vec()}
]}

wcp_snapshot_with_commands! {add_scope, vec![
    WcpCommand::Load{source: "../examples/counter.vcd".to_string()},
    WcpCommand::AddScope{scope: "tb".to_string()}
]}

wcp_snapshot_with_commands! {color_variables, vec![
    WcpCommand::Load{source:  "../examples/counter.vcd".to_string()},
    WcpCommand::AddScope{scope: "tb".to_string()},
    WcpCommand::SetItemColor{id:"3".to_string(), color:"GRAY".to_string()},
    WcpCommand::SetItemColor{id:"1".to_string(), color:"BLUE".to_string()},
    WcpCommand::SetItemColor{id:"2".to_string(), color:"YELLOW".to_string()}
]}

wcp_snapshot_with_commands! {remove_2_variables, vec![
    WcpCommand::Load{source: "../examples/counter.vcd".to_string()},
    WcpCommand::AddScope{scope: "tb".to_string()},
    WcpCommand::RemoveItems{ids: vec!["1".to_string(), "2".to_string()]},
]}

wcp_snapshot_with_commands! {focus_item, vec![
    WcpCommand::Load{source: "../examples/counter.vcd".to_string()},
    WcpCommand::AddScope{scope: "tb".to_string()},
    WcpCommand::FocusItem{id: "2".to_string()}
]}

wcp_snapshot_with_commands! {clear, vec![
    WcpCommand::Load{source: "../examples/counter.vcd".to_string()},
    WcpCommand::AddScope{scope: "tb".to_string()},
    WcpCommand::Clear
]}

wcp_snapshot_with_commands! {set_viewport_to, vec![
    WcpCommand::Load{source: "../examples/counter.vcd".to_string()},
    WcpCommand::AddScope{scope: "tb".to_string()},
    WcpCommand::SetViewportTo { timestamp: BigInt::from(710) },
]}
