use color_eyre::eyre::Result;
use eframe::egui::Context;
use serde::Deserialize;
use serde_json::Error as serde_Error;
use std::{
    io::prelude::*,
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
    time::Duration,
};

use log::info;

use super::wcp_handler::WcpMessage;

pub struct WcpHttpServer {
    listener: TcpListener,
    sender: Sender<WcpMessage>,
    receiver: Receiver<WcpMessage>,
    ctx: Arc<Context>,
}

impl WcpHttpServer {
    pub fn new(
        address: String,
        s2c_sender: Sender<WcpMessage>,
        c2s_receiver: Receiver<WcpMessage>,
        ctx: Arc<Context>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(address)?;
        info!(
            "WCP Server listening on port {}",
            listener.local_addr().unwrap()
        );
        Ok(WcpHttpServer {
            listener,
            sender: s2c_sender,
            receiver: c2s_receiver,
            ctx,
        })
    }

    pub fn run(&mut self) {
        let commands: Vec<String> = "add goto cursor_set reload get_item_list color get_item_info"
            .split_whitespace()
            .map(|v| v.to_string())
            .collect();

        let greeting = WcpMessage::create_greeting(0, commands);

        info!("WCP Listening on Port {:#?}", self.listener);
        let listener = self.listener.try_clone().unwrap();

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    info!("WCP New connection: {}", stream.peer_addr().unwrap());

                    //send greeting
                    let _ = serde_json::to_writer(&stream, &greeting).expect("to writer failed");
                    let _ = stream.write(b"\0");
                    stream.flush().unwrap();

                    //handle connection from client
                    if let Err(error) = self.handle_client(stream) {
                        info!("client disconnected with error: {error:#?}")
                    }
                }
                Err(e) => println!("connection failed : \n {e}"),
            }
        }
    }

    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), serde_Error> {
        loop {
            //get message from client
            let msg: WcpMessage = match self.get_json_message(&stream) {
                Ok(msg) => msg,
                Err(e) => {
                    match e.classify() {
                        //error when the client disconnects
                        serde_json::error::Category::Eof => return Err(e),
                        //if different error get next message and send error
                        _ => {
                            info!("WCP S>C error\n");

                            let _ = serde_json::to_writer(
                                &stream,
                                &WcpMessage::create_error(
                                    "parsing error".to_string(),
                                    vec![],
                                    "parsing error".to_string(),
                                ),
                            );
                            continue;
                        }
                    }
                }
            };

            info!("WCP C>S {:#?}\n", msg);
            //handle message
            let _ = self.sender.send(msg);
            // request repaint of the Surfer UI
            self.ctx.request_repaint();
            let resp = match self.receiver.recv_timeout(Duration::from_secs(10)) {
                Ok(resp) => resp,
                _ => WcpMessage::create_error(
                    "No response".to_string(),
                    vec![],
                    "No response from Surfer".to_string(),
                ),
            };
            info!("WCP S>C {:#?}\n", resp);
            //send response back to client
            serde_json::to_writer(&stream, &resp)?;
            let _ = stream.write(b"\0");
            let _ = stream.flush();
        }
    }

    fn get_json_message(&mut self, mut stream: &TcpStream) -> Result<WcpMessage, serde_Error> {
        let mut de = serde_json::Deserializer::from_reader(&mut stream);
        WcpMessage::deserialize(&mut de)
    }
}
