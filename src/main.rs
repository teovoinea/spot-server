extern crate websocket;
extern crate hyper;


use std::thread;
use std::io::Write;
use websocket::{Message, OwnedMessage};
use websocket::sync::Server;
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Response, const_service, service_fn};

const HTML: &'static str = include_str!("websockets.html");

fn main() {
	// Start listening for http connections
	thread::spawn(move || {
		                    let addr = ([127, 0, 0, 1], 3000).into();

                            let new_service = const_service(service_fn(|_| {
                                Ok(Response::<hyper::Body>::new()
                                    .with_header(ContentLength(HTML.len() as u64))
                                    .with_header(ContentType::html())
                                    .with_body(HTML))
                            }));

                            let server = Http::new()
                                .sleep_on_errors(true)
                                .bind(&addr, new_service)
                                .unwrap();
                            println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
                            server.run().unwrap();
		             });

	// Start listening for WebSocket connections
	let ws_server = Server::bind("127.0.0.1:2794").unwrap();

	for connection in ws_server.filter_map(Result::ok) {
		// Spawn a new thread for each connection.
		thread::spawn(move || {
			if !connection.protocols().contains(&"rust-websocket".to_string()) {
				connection.reject().unwrap();
				return;
			}

			let mut client = connection.use_protocol("rust-websocket").accept().unwrap();

			let ip = client.peer_addr().unwrap();

			println!("Connection from {}", ip);

			let message = Message::text("Hello");
			client.send_message(&message).unwrap();

			let (mut receiver, mut sender) = client.split().unwrap();

			for message in receiver.incoming_messages() {
				let message = message.unwrap();

				match message {
					OwnedMessage::Close(_) => {
						let message = Message::close();
						sender.send_message(&message).unwrap();
						println!("Client {} disconnected", ip);
						return;
					}
					OwnedMessage::Ping(data) => {
						let message = Message::pong(data);
						sender.send_message(&message).unwrap();
					}
					_ => sender.send_message(&message).unwrap(),
				}
			}
		});
    }
}