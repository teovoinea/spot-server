# spot-server

The server for running the Spot game.

## Running

```bash
git clone https://github.com/teovoinea/spot-server
cargo run --release
```

## Architecture

### rust-websocket

Broadcasts messages between websocket connections using a custom multi producer multi consumer channel scheme