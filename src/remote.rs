use mio::Sender as MioSender;
use mio::Token;
use std::result::Result;
use std::sync::mpsc::{Receiver};

use Protocol;
use FrameHandler;
use Codec;
use EventedByteStream;
use Command;

pub struct FrameEngineRemote<P: ?Sized> where
  P: Protocol {
    command_sender: MioSender<Command<P>>,
    receiver: Receiver<()>
}

impl <P: ?Sized> FrameEngineRemote<P> where
  P: Protocol {

  pub fn new(command_sender: MioSender<Command<P>>, receiver: Receiver<()>) -> FrameEngineRemote<P> {
    FrameEngineRemote {
      command_sender: command_sender,
      receiver: receiver
    }
  }

  //TODO: Error reporting
  pub fn send(&self, token: Token, frame: P::Frame) {
    let _ = self.command_sender.send(Command::Send(token, frame));
  }

  pub fn manage(&self, evented_byte_stream: P::ByteStream) {
    let _ = self.command_sender.send(Command::Manage(evented_byte_stream));
  }

  pub fn shutdown(&self) {
    let _ = self.command_sender.send(Command::Shutdown);
  }

  pub fn wait(&self) {
    let _ = self.receiver.recv();
  }
}
