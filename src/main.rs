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
use std::sync::{Arc, Mutex, mpsc, RwLock};
use std::sync::mpsc::{Sender, Receiver};
use std::u8;
use std::time;
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

	{ //initialize the panel to white
		PANEL.lock().unwrap().pixels_mut().map(|p| {
			p.data = [u8::MAX, u8::MAX, u8::MAX];
		}).collect::<Vec<_>>();
	}

	thread::spawn(move || {
		for broadcast_pixel in init_broadcast_receiver.iter() {
			let temp_pixel: Rgb<u8> = Rgb {
				data: [broadcast_pixel.r, broadcast_pixel.g, broadcast_pixel.b]
			};
			PANEL.lock().unwrap().put_pixel(broadcast_pixel.x as u32, broadcast_pixel.y as u32, temp_pixel);

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

			let terminator: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));

			let client = connection.use_protocol("rust-websocket").accept().unwrap();

			let ip = client.peer_addr().unwrap();

			println!("Connection from {}", ip);

			let (mut receiver, sender) = client.split().unwrap();
			let (sender_chan_sender, sender_chan_recv): (Sender<Message>, Receiver<Message>) = mpsc::channel();

			let panel_clone;
			{
				panel_clone = PANEL.lock().unwrap().clone();
			} // keep the clone but drop the lock

			panel_clone.enumerate_pixels().map(|(x, y, p)| {
				if p.data != [u8::MAX,u8::MAX,u8::MAX] { //don't send white pixel on the initial connection because canvas is already white
					let tmp_pixel = PaintPixel {
						x: x as i32,
						y: y as i32,
						r: p.data[0],
						g: p.data[1],
						b: p.data[2]
					};
					//TODO(teo): error handling
					sender_chan_sender.send(Message::binary(serialize(&tmp_pixel).unwrap()));
				}
			}).collect::<Vec<_>>();
			
			let t = Arc::clone(&terminator);
			let sender_thread = thread::spawn(|| sending(t, sender_chan_recv, sender));

			let sender_chan_sender2 = sender_chan_sender.clone();
			let t2 = Arc::clone(&terminator);
			let broadcast_thread = thread::spawn(|| broadcasting(t2, send_broadcast, sender_chan_sender2));
			
			for message in receiver.incoming_messages() {
				let message = message.unwrap();
				match message {
					OwnedMessage::Close(_) => {
						{	
							let mut t = terminator.write().unwrap();
							*t = true;
						} //drop the lock

						//TODO(teo): error handling
						sender_thread.join();
						broadcast_thread.join();
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

fn sending(term: Arc<RwLock<bool>>, sender_chan: mpsc::Receiver<websocket::Message>, mut web_sender: websocket::sender::Writer<std::net::TcpStream>) {
	let wait = time::Duration::from_millis(50);
	loop {
		if *term.read().unwrap() {
			println!("Terminating sender thread");
			return;
		}
		if let Ok(send_message) = sender_chan.try_recv() {
			web_sender.send_message(&send_message).unwrap();
		}
		else if cfg!(feature = "low_cpu"){
			thread::sleep(wait);
		}
	}
}

fn broadcasting(term: Arc<RwLock<bool>>, receiver: spmc::Receiver<PaintPixel>, sender: mpsc::Sender<websocket::Message>) {
	let wait = time::Duration::from_millis(50);
	loop {
		if *term.read().unwrap() == true {
			println!("Terminating broadcast thread");
			return;
		}
		if let Ok(broadcast_pixel) = receiver.try_recv() {
			let s = sender.send(Message::binary(serialize(&broadcast_pixel).unwrap()));
			match s {
				Ok(_) => {},
				//This error hits, need to find out why...
				Err(SendError(e)) => println!("Failed to send broadcasted pixel to client {:?}", e)
			}
		}
		else if cfg!(feature = "low_cpu") {
			thread::sleep(wait);
		}
	}
}