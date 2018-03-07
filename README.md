[![Build Status](https://travis-ci.org/teovoinea/spot-server.svg?branch=master)](https://travis-ci.org/teovoinea/spot-server)
[![Build status](https://ci.appveyor.com/api/projects/status/h0n4s5ws4fqwjbne?svg=true)](https://ci.appveyor.com/project/teovoinea/spot-server)


# spot-server

The server for running the Spot game.

## Running

```bash
git clone https://github.com/teovoinea/spot-server
cargo run --release
```

**OR** (soon)

```bash
cargo install spot-server
cargo run spot-server
```

## Architecture

### rust-websocket

Broadcasts messages between websocket connections using a custom multi producer multi consumer channel scheme
