#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate serde_derive;
extern crate env_logger;
extern crate ws;
extern crate image;
extern crate bincode;

use std::thread;
use std::sync::{Arc, Mutex};
use std::u8;

use ws::{listen, CloseCode, Sender, Handler, Message, Result, Handshake};
use image::{ImageBuffer, Rgb};
use bincode::{serialize, deserialize};

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

struct Server {
	out: Sender,
}

fn main() {
	env_logger::init();

	{ //initialize the panel to white
		PANEL.lock().unwrap().pixels_mut().map(|p| {
			p.data = [u8::MAX, u8::MAX, u8::MAX];
		}).collect::<Vec<_>>();
	}

	// Server thread
    let server = thread::spawn(move || {
        listen("127.0.0.1:2794", |out| {

            Server { out: out }

        }).unwrap()
	});

	let _ = server.join();
}

impl Handler for Server {

	fn on_open(&mut self, _: Handshake) -> Result<()> {
		let panel_clone;
		{
			panel_clone = PANEL.lock().unwrap().clone();
		}
		panel_clone.enumerate_pixels().map(|(x, y, p)| {
			if p.data != [u8::MAX,u8::MAX,u8::MAX] { //don't send white pixel on the initial connection because canvas is already white
				let tmp_pixel = PaintPixel {
					x: x as i32,
					y: y as i32,
					r: p.data[0],
					g: p.data[1],
					b: p.data[2]
				};
				match self.out.send(Message::binary(serialize(&tmp_pixel).unwrap())) {
					Ok(_) => {}
					Err(e) => {
						error!("Error sending initial panel to client {:?}", e);
					}
				}
			}
		}).collect::<Vec<_>>();
		Ok(())
	}

	fn on_message(&mut self, msg: Message) -> Result<()> {
		let msg_clone = msg.clone();
		let new_pixel : PaintPixel = deserialize(&msg.into_data()).unwrap();
		if new_pixel.x < 640 && new_pixel.y < 480 {
			match self.out.broadcast(msg_clone) {
				Ok(_) => {}
				Err(e) => error!("Failed to broadcast pixel {:?}", e)
			};
			let temp_pixel: Rgb<u8> = Rgb {
				data: [new_pixel.r, new_pixel.g, new_pixel.b]
			};
			{
				PANEL.lock().unwrap().put_pixel(new_pixel.x as u32, new_pixel.y as u32, temp_pixel);
			}
		}
		Ok(())
	}

	fn on_close(&mut self, code: CloseCode, reason: &str) {
		debug!("WebSocket closing for ({:?}) {}", code, reason);
	}
}