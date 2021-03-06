use std::thread::spawn;
use std::mem::transmute;
use std::sync::mpsc::{Sender, SendError, Receiver, RecvError, channel};
use super::{ChannelSend, ChannelRecv, Carrier, HasDual, Chan};

pub struct Channel {
    tx: Sender<Box<u8>>,
    rx: Receiver<Box<u8>>,
}

#[derive(Clone, Debug)]
pub struct Value<T>(pub T) where T: Send + 'static;

impl<T> ChannelSend for Value<T> where T: Send + 'static {
    type Crr = Channel;
    type Err = SendError<Box<T>>;

    fn send(self, carrier: &mut Self::Crr) -> Result<(), Self::Err> {
        unsafe {
            let tx: &Sender<Box<T>> = transmute(&carrier.tx);
            tx.send(Box::new(self.0))
        }
    }
}

impl<T> ChannelRecv for Value<T> where T: Sized + Send + 'static {
    type Crr = Channel;
    type Err = RecvError;

    fn recv(carrier: &mut Self::Crr) -> Result<Self, Self::Err> {
        unsafe {
            let rx: &Receiver<Box<T>> = transmute(&carrier.rx);
            rx.recv().map(|v| Value(*v))
        }
    }
}

impl Carrier for Channel {
    type SendChoiceErr = SendError<Box<bool>>;
    fn send_choice(&mut self, choice: bool) -> Result<(), Self::SendChoiceErr> {
        Value(choice).send(self)
    }

    type RecvChoiceErr = RecvError;
    fn recv_choice(&mut self) -> Result<bool, Self::RecvChoiceErr> {
        Value::recv(self).map(|Value(value)| value)
    }
}

/// Returns two session channels
#[must_use]
pub fn session_channel<P: HasDual>() -> (Chan<Channel, (), P>, Chan<Channel, (), P::Dual>) {
    let (master_tx, slave_rx) = channel();
    let (slave_tx, master_rx) = channel();

    let master_carrier = Channel {
        tx: master_tx,
        rx: master_rx,
    };
    let slave_carrier = Channel {
        tx: slave_tx,
        rx: slave_rx,
    };

    (Chan::new(master_carrier),
     Chan::new(slave_carrier))
}

/// Connect two functions using a session typed channel.
pub fn connect<FM, FS, P>(master_fn: FM, slave_fn: FS) where
    FM: Fn(Chan<Channel, (), P>) + Send,
    FS: Fn(Chan<Channel, (), P::Dual>) + Send + 'static,
    P: HasDual + Send + 'static,
    <P as HasDual>::Dual: HasDual + Send + 'static
{
    let (master, slave) = session_channel();
    let thread = spawn(move || slave_fn(slave));
    master_fn(master);
    thread.join().unwrap();
}
