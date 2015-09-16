# mai
A thin I/O layer built on top of [`mio`](https://github.com/carllerche/mio) that manages buffers and streams so you can focus
on sending and receiving your protocol's frames. If you're writing a client or
server for on top of TCP, this is the library for you.

## Status
Largely functional but currently pre-alpha.

## Getting Started

Using `mai` requires three steps:

* Selecting a data type to be your protocol's `Frame`, an actionable message.
* Defining a `Codec` that knows how to read and write `Frame`s into byte buffers.
* Specifying a `FrameHandler` to react to new connections, incoming `Frame`s and errors.

Buffer pooling, low-level `reads` and `writes` and `Token` management are handled by `mai`.

## An Echo Client example

### Protocol 

Implement the `Protocol` trait by specifying the family of types you'll be using.

```rust
use mai::*;

struct EchoCodec;
struct EchoClientHandler;
struct EchoClient;

impl Protocol for EchoClient {
  type ByteStream = TcpStream; // vs a UnixStream, for example
  type Frame = String;
  type Codec = EchoCodec;
  type Handler = EchoClientHandler;
  type Timeout = usize;
}
```

### Codec
Define methods to encode and decode your frames. Use the return codes to indicate that you got a frame, don't have enough bytes to read a frame yet or that you encountered a protocol error.

```rust

// For a simple Echo server, we can use `String` as our Frame type.
// This codec would work for both a client and server connection.
impl Codec<String> for EchoCodec {
  // Provide a method to try to write a given frame to a byte buffer
  fn encode(&mut self, message: &String, buffer: &mut [u8]) -> EncodingResult {
    let bytes = message.as_bytes();
    // If the buffer isn't big enough, say so via the return value
    if bytes.len() > buffer.len() {
      return Err(EncodingError::InsufficientBuffer);
    }
    // Copy the bytes of our String into the buffer
    for (index, &byte) in bytes.iter().enumerate() {
        buffer[index] = byte;
    }
    // Tell the frame engine how many bytes we wrote
    Ok(BytesWritten(bytes.len()))
  }

  // Provide a method to try to parse a frame from a byte buffer
  fn decode(&mut self, buffer: &[u8]) -> DecodingResult<String> {
    use std::str;
    // Validate that the buffer contains a utf-8 String
    let message: String = match str::from_utf8(buffer) {
      Ok(message) => message.to_owned(),
      // For this example, assume that an invalid message means 
      // that we just don't have enough bytes yet
      Err(error) => return Err(DecodingError::IncompleteFrame)
    };
    Ok(DecodedFrame::new(message, BytesRead(buffer.len())))
  }
}
```

### FrameHandler
Define callbacks to handle byte stream events: connections, frames, timeouts, errors, and disconnects.
```rust
use mai::*;

impl FrameHandler<EchoClient> for EchoClientHandler {
  fn on_ready(&mut self, stream: &mut FrameStream<EchoClient>) {
    println!("Connected to {:?}, issued {:?}", stream.peer_addr(), stream.token());
    let message: String = "Supercalifragilisticexpialidocious!".to_owned();
    println!("Sending message...");
    stream.send(message);
    stream.timeout_ms(55, 5_000);
  }
  fn on_frame(&mut self, stream: &mut FrameStream<EchoClient>, message: String) {
    println!("Received a message from {:?}/{:?}: '{}'", stream.peer_addr(), stream.token(), &message.trim_right());
  }
  fn on_timeout(&mut self, timeout: usize) {
    println!("A timeout has occurred: {:?}", timeout);
  }
  fn on_error(&mut self, stream: &mut FrameStream<EchoClient>, error: Error) {
    println!("Error. {:?}/{:?}, {:?}", stream.peer_addr(), stream.token(), error);
  }
  fn on_closed(&mut self, stream: &FrameStream<EchoClient>) {
    println!("Disconnected from {:?}/{:?}", stream.peer_addr(), stream.token());
  }
}
```

### Get to work
Create a `FrameEngine` and hand it any `mio` type that is `Evented`+`Read`+`Write`. Watch it go!
```rust
fn main() {
  // Create a TcpStream connected to `nc` running as an echo server
  // nc -l -p 2000 -c 'xargs -n1 echo'
  println!("Connecting to localhost:9999...");
  let address = "0.0.0.0:9999".parse().unwrap();
  let socket = TcpSocket::v4().unwrap();
  let (stream, _complete) = socket.connect(&address).unwrap();
  
  // Hand the TcpStream off to our new `FrameEngine` configured to treat its
  // byte streams as Echo clients.
  let frame_engine: FrameEngineRemote<EchoClient> = mai::frame_engine(EchoCodec, EchoClientHandler).run();
  frame_engine.manage(stream);
  frame_engine.wait();
}
```
