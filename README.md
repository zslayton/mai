# mai
A higher-level event loop built on top of [`mio`](https://github.com/carllerche/mio). `mai` manages buffers and streams so you can focus on sending and receiving your protocol's frames.

## Status
Largely functional. APIs subject to change.

## Getting Started

Using `mai` requires three steps:

* Selecting a data type to be your protocol's `Frame`, an actionable message.
* Defining a `Codec` that knows how to read and write `Frame`s into byte buffers.
* Specifying a `Handler` to react to new connections, incoming `Frame`s and errors.

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

impl Handler<EchoClient> for EchoClientHandler {
  fn on_ready(&mut self, context: &mut Context<EchoClient>) {
    let stream = context.stream();
    println!("Connected to {:?}", stream.peer_addr());
    let message: String = "Supercalifragilisticexpialidocious!".to_owned();
    stream.send(message);
  }
  fn on_frame(&mut self, stream: &mut Context<EchoClient>, message: String) {
    let stream = context.stream();
    println!("Received a message from {:?}: '{}'", stream.peer_addr(), &message.trim_right());
  }
  fn on_timeout(&mut self, timeout: usize) {
    println!("A timeout has occurred: {:?}", timeout);
  }
  fn on_error(&mut self, context: &mut Context<EchoClient>, error: &Error) {
    let stream = context.stream();
    println!("Error. {:?}, {:?}", stream.peer_addr(), error);
  }
  fn on_closed(&mut self, stream: &Context<EchoClient>) {
    let stream = context.stream();
    println!("Disconnected from {:?}", stream.peer_addr());
  }
}
```

### Get to work
Create a `ProtocolEngine` and hand it any `mio` type that is `Evented`+`Read`+`Write`. Watch it go!
```rust
fn main() {
  // Create a TcpStream connected to `nc` running as an echo server
  // nc -l -p 2000 -c 'xargs -n1 echo'
  println!("Connecting to localhost:9999...");
  let address = "0.0.0.0:9999".parse().unwrap();
  let socket = TcpSocket::v4().unwrap();
  let (stream, _complete) = socket.connect(&address).unwrap();
  
  // Hand the TcpStream off to our new `ProtocolEngine` configured to treat its
  // byte streams as Echo clients.
  let protocol_engine: ProtocolEngine<EchoClient> = mai::protocol_engine(EchoClientHandler)
    .with(InitialBufferSize(Kilobytes(32))
    .with(InitialBufferPoolSize(16))
    .with(MaxBufferPoolSize(128))
    .build();
  let token = protocol_engine.manage(stream);
  let _ = protocol_engine.wait();
}
```

## Creating a Server

Currently `mai` does not have a built-in way to manage incoming connections. This is [being worked on](https://github.com/zslayton/mai/issues/7).

Creating a server is conceptually a straightforward process: create a separate thread using mio to listen for incoming connections. Each time a client connection is avialable, pass the corresponding TcpStream to the ProtocolEngine running on a separate thread.
