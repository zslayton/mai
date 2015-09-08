use mio::Sender as MioSender;
use mio::Token;
use std::result::Result;
use std::sync::mpsc::{Receiver};

use FrameHandler;
use Codec;
use EventedByteStream;
use Command;

pub struct FrameEngineRemote<E, F> where
  E: EventedByteStream,
  F: Send {
    command_sender: MioSender<Command<E,F>>,
    receiver: Receiver<()>
}

impl <E, F> FrameEngineRemote<E, F> where
  E: EventedByteStream,
  F: Send {

  pub fn new(command_sender: MioSender<Command<E,F>>, receiver: Receiver<()>) -> FrameEngineRemote<E, F> {
    FrameEngineRemote {
      command_sender: command_sender,
      receiver: receiver
    }
  }

  //TODO: Error reporting
  pub fn send(&self, token: Token, frame: F) {
    let _ = self.command_sender.send(Command::Send(token, frame));
  }

  pub fn manage(&self, evented_byte_stream: E) {
    let _ = self.command_sender.send(Command::Manage(evented_byte_stream));
  }

  pub fn shutdown(&self) {
    let _ = self.command_sender.send(Command::Shutdown);
  }

  pub fn wait(&self) {
    let _ = self.receiver.recv();
  }
}
