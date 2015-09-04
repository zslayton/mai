use mio::Evented;
use std::io::{self, Read, Write, ErrorKind};

use Buffer;

pub trait EventedByteStream : Evented + Read + Write {
  fn on_ready(&mut self);
  fn on_readable(&mut self, &mut Buffer) -> io::Result<usize>;
  fn on_writable(&mut self, &mut Buffer) -> io::Result<usize>;
  fn on_hup(&mut self);
  fn on_error(&mut self);
}

impl <T> EventedByteStream for T where T: Evented + Read + Write {
  fn on_ready(&mut self) {
    debug!("EventedByteStream is now ready");
  }

  fn on_readable(&mut self, buffer: &mut Buffer) -> io::Result<usize> {
    debug!("Reading from EventedByteStream");
    let mut total_bytes_read: usize = 0;
    loop {
      buffer.set_size(total_bytes_read);
      let raw_buffer: &mut [u8] = buffer.remaining();
      if raw_buffer.len() == 0 {
        debug!("Buffer is full. Yielding.");
        return Ok(total_bytes_read);
      }
      debug!("Invoking read(), buffer has {} bytes free", raw_buffer.len());
      match self.read(raw_buffer) {
        Ok(bytes_read) => {
          debug!("Read {} bytes", bytes_read);
          if bytes_read == 0 {
            return Ok(total_bytes_read);
          }
          total_bytes_read += bytes_read;
        },
        Err(error) => {
          if error.kind() == ErrorKind::WouldBlock {
            debug!("Read error WouldBlock, yielding.");
            return Ok(total_bytes_read);
          }
          debug!("A differeng kind of error happened: {:?}", error);
          return Err(error);
        }
      }
    }
  }

  fn on_writable(&mut self, buffer: &mut Buffer) -> io::Result<usize> {
    debug!("Writing to bytestream");
    let mut total_bytes_written: usize = 0;
    loop {
      let working_buffer: &[u8] = &buffer.bytes()[total_bytes_written..];
      if working_buffer.len() == 0 {
        debug!("Buffer is empty. Yielding.");
        break;
      }
      match self.write(working_buffer) {
        Ok(bytes_written) => {
          debug!("Wrote {} bytes", bytes_written);
          total_bytes_written += bytes_written;
        },
        Err(error) => {
          if error.kind() == ErrorKind::WouldBlock {
            debug!("Write error WouldBlock, yielding.");
            break;
          }
          panic!("Unknown error while writing: {:?}", error); //TODO: Graceful handling
        }
      }
    }
    match self.flush() {
      Ok(_) => {},
      Err(error) => {
        error!("Could not flush write stream: {:?}", error);
      }
    };
    buffer.restack(total_bytes_written);
    return Ok(total_bytes_written);
  }
  
  fn on_hup(&mut self) {
    debug!("EventedByteStream hup");
  }

  fn on_error(&mut self) {
    debug!("EventedByteStream error");
  }
}
