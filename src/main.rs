extern crate websocket;
extern crate hyper;
extern crate image;
#[macro_use]
extern crate serde_derive;
extern crate bincode;
#[macro_use]
extern crate lazy_static;
extern crate spmc;

use std::thread;
use std::io::Write;
use std::sync::{Arc, Mutex, mpsc};
use std::sync::mpsc::{Sender, Receiver};
use websocket::{Message, OwnedMessage};
use websocket::sync::Server;
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Http, Response, const_service, service_fn};
use image::{GenericImage, ImageBuffer, Rgb};
use bincode::{serialize, deserialize};

const HTML: &'static str = include_str!("websockets.html");

lazy_static! {
	static ref PANEL: Arc<Mutex<ImageBuffer<Rgb<u8>, Vec<u8>>>> = Arc::new(Mutex::new(ImageBuffer::new(640, 480)));
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct PaintPixel {
	x: i32,
	y: i32,
	r: u8,
	g: u8,
	b: u8,
}

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
	let (init_broadcast_sender, init_broadcast_receiver): (Sender<PaintPixel>, Receiver<PaintPixel>) = mpsc::channel();
	let (complete_broadcast_sender, complete_broadcast_receiver): (spmc::Sender<PaintPixel>, spmc::Receiver<PaintPixel>) = spmc::channel();

	thread::spawn(move || {
		for broadcast_pixel in init_broadcast_receiver.iter() {
			//TODO: error handling
			let _ = complete_broadcast_sender.send(broadcast_pixel);
		}
	});

	for connection in ws_server.filter_map(Result::ok) {
		let new_broadcast = init_broadcast_sender.clone();
		let send_broadcast = complete_broadcast_receiver.clone();
		// Spawn a new thread for each connection.
		thread::spawn(move || {
			if !connection.protocols().contains(&"rust-websocket".to_string()) {
				connection.reject().unwrap();
				return;
			}

			let mut client = connection.use_protocol("rust-websocket").accept().unwrap();

			let ip = client.peer_addr().unwrap();

			println!("Connection from {}", ip);

			//let message = Message::text("Hello");
			//client.send_message(&message).unwrap();

			let (mut receiver, mut sender) = client.split().unwrap();
			let (sender_chan_sender, sender_chan_recv): (Sender<Message>, Receiver<Message>) = mpsc::channel();

			thread::spawn(move || {
				for send_message in sender_chan_recv.iter() {
					sender.send_message(&send_message).unwrap();
				}
			});

			let sender_chan_sender2 = sender_chan_sender.clone();
			thread::spawn(move || {
				loop {
					if let Ok(broadcast_pixel) = send_broadcast.try_recv() {
						sender_chan_sender2.send(Message::binary(serialize(&broadcast_pixel).unwrap()));
					}
				}
			});
			
			for message in receiver.incoming_messages() {
				let message = message.unwrap();

				match message {
					OwnedMessage::Close(_) => {
						let message = Message::close();
						sender_chan_sender.send(message);
						println!("Client {} disconnected", ip);
						return;
					}
					OwnedMessage::Ping(data) => {
						let message = Message::pong(data);
						sender_chan_sender.send(message);
					}
					OwnedMessage::Binary(data) => {
						let new_pixel : PaintPixel = deserialize(&data).unwrap();
						if new_pixel.x <= 640 && new_pixel.y <= 480 {
							let mut img = PANEL.lock().unwrap();
							let temp_pixel: Rgb<u8> = Rgb {
								data: [new_pixel.r, new_pixel.g, new_pixel.b]
							};
							img.put_pixel(new_pixel.x as u32, new_pixel.y as u32, temp_pixel);
							sender_chan_sender.send(Message::binary(data));
							//TODO: error handling
							let _ = new_broadcast.send(new_pixel);
						}
					}
					_ => {
						println!("Got some other data {:?}", message);
					}
				}
			}
		});
    }
}