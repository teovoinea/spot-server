extern crate websocket;
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
use std::u8;
use websocket::{Message, OwnedMessage};
use websocket::sync::Server;
use image::{GenericImage, ImageBuffer, Rgb};
use bincode::{serialize, deserialize};
use spmc::SendError;

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
	// Start listening for WebSocket connections
	let ws_server = Server::bind("127.0.0.1:2794").unwrap();
	let (init_broadcast_sender, init_broadcast_receiver): (Sender<PaintPixel>, Receiver<PaintPixel>) = mpsc::channel();
	let (complete_broadcast_sender, complete_broadcast_receiver): (spmc::Sender<PaintPixel>, spmc::Receiver<PaintPixel>) = spmc::channel();

	{
		let mut img = PANEL.lock().unwrap();
		img.pixels_mut().map(|p| {
			p.data = [u8::MAX, u8::MAX, u8::MAX];
		}).collect::<Vec<_>>();
	}

	thread::spawn(move || {
		for broadcast_pixel in init_broadcast_receiver.iter() {
			let b = complete_broadcast_sender.send(broadcast_pixel);
			match b {
				Ok(_) => {},
				Err(e) => println!("Failed to brodcast pixel from main thread {:?}", e)
			}
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

			let client = connection.use_protocol("rust-websocket").accept().unwrap();

			let ip = client.peer_addr().unwrap();

			println!("Connection from {}", ip);

			let (mut receiver, mut sender) = client.split().unwrap();
			let (sender_chan_sender, sender_chan_recv): (Sender<Message>, Receiver<Message>) = mpsc::channel();

			let panel_clone;
			{
				let mut img = PANEL.lock().unwrap();
				panel_clone = img.clone();
			} //img is dropped here and panel is unlocked

			panel_clone.enumerate_pixels().map(|(x, y, p)| {
				if p.data != [u8::MAX,u8::MAX,u8::MAX] {
					let tmp_pixel = PaintPixel {
						x: x as i32,
						y: y as i32,
						r: p.data[0],
						g: p.data[1],
						b: p.data[2]
					};
					sender_chan_sender.send(Message::binary(serialize(&tmp_pixel).unwrap()));
				}
			}).collect::<Vec<_>>();

			thread::spawn(move || {
				for send_message in sender_chan_recv.iter() {
					sender.send_message(&send_message).unwrap();
				}
			});

			let sender_chan_sender2 = sender_chan_sender.clone();
			thread::spawn(move || {
				loop {
					if let Ok(broadcast_pixel) = send_broadcast.try_recv() {
						let s = sender_chan_sender2.send(Message::binary(serialize(&broadcast_pixel).unwrap()));
						match s {
							Ok(_) => {},
							//This error hits, need to find out why...
							Err(SendError(e)) => println!("Failed to send broadcasted pixel to client {:?}", e)
						}
					}
				}
			});
			
			for message in receiver.incoming_messages() {
				let message = message.unwrap();

				match message {
					OwnedMessage::Close(_) => {
						let message = Message::close();
						let d = sender_chan_sender.send(message);
						match d {
							Ok(_) => {}
							Err(e) => println!("Failed to send close message {:?}", e)
						};
						println!("Client {} disconnected", ip);
						return;
					}
					OwnedMessage::Ping(data) => {
						let message = Message::pong(data);
						match sender_chan_sender.send(message) {
							Ok(_) => {}
							Err(e) => println!("Failed to send ping message {:?}", e)
						};
					}
					OwnedMessage::Binary(data) => {
						let new_pixel : PaintPixel = deserialize(&data).unwrap();
						if new_pixel.x < 640 && new_pixel.y < 480 {
							let mut img = PANEL.lock().unwrap();
							let temp_pixel: Rgb<u8> = Rgb {
								data: [new_pixel.r, new_pixel.g, new_pixel.b]
							};
							img.put_pixel(new_pixel.x as u32, new_pixel.y as u32, temp_pixel);
							match sender_chan_sender.send(Message::binary(data)) {
								Ok(_) => {}
								Err(e) => println!("Failed to send received pixel to sender thread {:?}", e)
							};
							match new_broadcast.send(new_pixel) {
								Ok(_) => {}
								Err(e) => println!("Failed to send received pixel to broadcaster thread {:?}", e)
							};
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