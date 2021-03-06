use mio::Sender as MioSender;
use mio::Token;
use std::sync::mpsc::{Receiver};

use Protocol;
use Command;

pub struct ProtocolEngineRemote<P: ?Sized> where
  P: Protocol {
    command_sender: MioSender<Command<P>>,
    receiver: Receiver<()>
}

impl <P: ?Sized> ProtocolEngineRemote<P> where
  P: Protocol {

  pub fn new(command_sender: MioSender<Command<P>>, receiver: Receiver<()>) -> ProtocolEngineRemote<P> {
    ProtocolEngineRemote {
      command_sender: command_sender,
      receiver: receiver
    }
  }

  //TODO: Error reporting
  pub fn send(&self, token: Token, frame: P::Frame) {
    let _ = self.command_sender.send(Command::Send(token, frame));
  }

  pub fn manage(&self, evented_byte_stream: P::ByteStream, session: P::Session) {
    let _ = self.command_sender.send(Command::Manage(evented_byte_stream, session));
  }

  pub fn shutdown(&self) {
    let _ = self.command_sender.send(Command::Shutdown);
  }

  pub fn wait(&self) {
    let _ = self.receiver.recv();
  }
}
