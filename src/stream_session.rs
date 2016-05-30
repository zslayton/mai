use evented_frame_stream::EventedFrameStream;
use protocol::Protocol;

pub struct StreamSession<P: ?Sized> where P: Protocol {
  efs: EventedFrameStream<P>,
  session: P::Session
}

impl <P: ?Sized> StreamSession<P> where P: Protocol {
    pub fn new(efs: EventedFrameStream<P>, session: P::Session) -> StreamSession<P> {
        StreamSession {
            efs: efs,
            session: session
        }
    }

    pub fn components(&mut self) -> (&mut EventedFrameStream<P>, &mut P::Session) {
        let StreamSession {
            ref mut efs,
            ref mut session,
        } = *self;
        (efs, session)
    }

    pub fn stream(&mut self) -> &mut EventedFrameStream<P>{
        let StreamSession {
            ref mut efs,
            ..
        } = *self;
        efs
    }

    pub fn session(&mut self) -> &mut P::Session {
        let StreamSession {
            ref mut session,
            ..
        } = *self;
        session
    }
}
