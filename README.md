# mai
A thin I/O layer built on top of mio that manages buffers and streams so you can focus
on sending and receiving your protocol's frames.

## Status
Currently pre-alpha. Basic I/O and buffering are in place, but the API is in flux and error handling needs work.

## An Echo Client example

### Codec
Define methods to encode and decode your frames. Use the return codes to indicate that you got a frame, don't have enough bytes to read a frame yet or that you encountered a protocol error.

```rust
use mai::codec::*;

struct EchoCodec;

// For a simple Echo server, we can use `String` as our Frame type.
// This codec would work for both a client and server connection.
impl Codec<String> for EchoCodec {
  // Provide a method to write a given frame to a byte buffer
  fn encode(&mut self, message: &String, buffer: &mut [u8]) -> EncodingResult {
    let bytes = message.as_bytes();
    // Make sure the buffer is big enough
    if bytes.len() > buffer.len() {
      return Err(EncodingError::InsufficientBufferSize);
    }
    // Copy the bytes of our String into the buffer
    for (index, &byte) in bytes.iter().enumerate() {
        buffer[index] = byte;
    }
    // Tell the frame engine how many bytes we wrote
    Ok(BytesWritten(bytes.len()))
  }

  // Provide a method to try to read a frame from a byte buffer
  fn decode(&mut self, buffer: &[u8]) -> DecodingResult<String> {
    use std::str;
    // Validate that the buffer contains a utf-8 String
    let message: String = match str::from_utf8(buffer) {
      Ok(message) => message.to_owned(),
      // For this example, assume invalid messages means that we just don't have enough bytes yet
      Err(error) => return Err(DecodingError::IncompleteFrame)
    };
    Ok(DecodedFrame::new(message, BytesRead(buffer.len())))
  }
}
```

### FrameHandler
Define callbacks to handle frames that have been received and be notified that frames were successfully written.
```rust
use mai::FrameHandler;

struct EchoFrameHandler;

impl FrameHandler<String> for EchoFrameHandler {
  // We got a frame (String) from the echo server!
  fn on_frame_received(&mut self, message: String) {
    println!("Received a message: '{}'", &message.trim_right());
  }
  // Remember this frame (String) you sent asynchronously? It went out ok.
  fn on_frame_written(&mut self, message: String) {
    println!("Wrote a message: '{}'", &message.trim_right());
  }
}
```

### Get to work
Create a `FrameEngine` and hand it any `mio` type that is `Evented`+`Read`+`Write`. Watch it go!
```rust
extern crate mio;
extern crate mai;
extern crate env_logger;

use mio::tcp::{TcpSocket, TcpStream};

fn main() {
  env_logger::init().unwrap(); // Set up logging
  
  let mut frame_engine = mai::frame_engine(EchoCodec, EchoFrameHandler);
  
  println!("Connecting to 0.0.0.0:9999...");
  let address = "0.0.0.0:9999".parse().unwrap();
  let socket = TcpSocket::v4().unwrap();
  let (stream, _complete) = socket.connect(&address).unwrap();
  
  frame_engine.manage(stream);
  frame_engine.run();
}
```
