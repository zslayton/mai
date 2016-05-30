use mio::Evented;
use std::io::{self, Read, Write, ErrorKind};

use Buffer;

pub trait EventedByteStream : Evented + Read + Write + Send {
  fn read_into_buffer(&mut self, &mut Buffer) -> io::Result<usize>;
  fn write_from_buffer(&mut self, &mut Buffer) -> io::Result<usize>;
}

impl <T> EventedByteStream for T where T: Evented + Read + Write + Send {
  fn read_into_buffer(&mut self, buffer: &mut Buffer) -> io::Result<usize> {
    debug!("Reading from EventedByteStream");
    let mut bytes_available: usize = buffer.len(); // The buffer may have leftover data
    loop {
      buffer.set_size(bytes_available);
      let raw_buffer: &mut [u8] = buffer.remaining();
      if raw_buffer.len() == 0 {
        debug!("Buffer is full. Yielding.");
        break;
      }
      debug!("Invoking read(), buffer has {} bytes free", raw_buffer.len());
      match self.read(raw_buffer) {
        Ok(bytes_read) => {
          bytes_available += bytes_read;
          debug!("Read {} bytes, {} now available", bytes_read, bytes_available);
          if bytes_read == 0 {
            break;
          }
        },
        Err(error) => {
          if error.kind() == ErrorKind::WouldBlock {
            debug!("Read error WouldBlock, yielding.");
            break;
          }
          return Err(error);
        }
      }
    }

    return Ok(bytes_available);
  }

  fn write_from_buffer(&mut self, buffer: &mut Buffer) -> io::Result<usize> {
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
          return Err(error);
        }
      }
    }
/*    match self.flush() {
      Ok(_) => {},
      Err(error) => {
        error!("Could not flush write stream: {:?}", error);
        return Err(error);
      }
    };
    */
    buffer.restack(total_bytes_written);
    return Ok(total_bytes_written);
  }
}
