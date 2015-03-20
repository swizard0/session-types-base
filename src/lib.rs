//! rust-sessions
//!
//! This is an implementation of *session types* in Rust.
//!
//! The channels in Rusts standard library are useful for a great many things,
//! but they're restricted to a single type. Session types allows one to use a
//! single channel for transferring values of different types, depending on the
//! context in which it is used. Specifically, a session typed channel always
//! carry a *protocol*, which dictates how communication is to take place.
//!
//! For example, imagine that two threads, `A` and `B` want to communicate with
//! the following pattern:
//!
//!  1. `A` sends an integer to `B`.
//!  2. `B` sends a boolean to `A` depending on the integer received.
//!
//! With session types, this could be done by sharing a single channel. From
//! `A`'s point of view, it would have the type `int ! (bool ? eps)` where `t ! r`
//! is the protocol "send something of type `t` then proceed with
//! protocol `r`", the protocol `t ? r` is "receive something of type `t` then proceed
//! with protocol `r`, and `eps` is a special marker indicating the end of a
//! communication session.
//!
//! Our session type library allows the user to create channels that adhere to a
//! specified protocol. For example, a channel like the above would have the type
//! `Chan<(), Send<i64, Recv<bool, Eps>>>`, and the full program could look like this:
//!
//! ```
//! extern crate "rust-sessions" as sessions;
//! use sessions::*;
//!
//! type Server = Recv<i64, Send<bool, Eps>>;
//! type Client = Send<i64, Recv<bool, Eps>>;
//!
//! fn srv(c: Chan<(), Server>) {
//!     let (c, n) = c.recv();
//!     if n % 2 == 0 {
//!         c.send(true).close()
//!     } else {
//!         c.send(false).close()
//!     }
//! }
//!
//! fn cli(c: Chan<(), Client>) {
//!     let n = 42;
//!     let c = c.send(n);
//!     let (c, b) = c.recv();
//!
//!     if b {
//!         println!("{} is even", n);
//!     } else {
//!         println!("{} is odd", n);
//!     }
//!
//!     c.close();
//! }
//!
//! fn main() {
//!     connect(srv, cli);
//! }
//! ```


#![feature(std_misc)]

use std::marker;
use std::thread::scoped;
use std::mem::transmute;
use std::sync::mpsc::{Sender, Receiver, channel};
use std::collections::HashMap;
use std::sync::mpsc::Select;
use std::marker::{PhantomData, PhantomFn};

/// A session typed channel. `T` is the protocol and `E` is the environment,
/// containing potential recursion targets
pub struct Chan<E, T> (Sender<Box<u8>>, Receiver<Box<u8>>, PhantomData<(E, T)>);

fn unsafe_write_chan<A: marker::Send + 'static, E, T>
    (&Chan(ref tx, _, _): &Chan<E, T>, x: A)
{
    let tx: &Sender<Box<A>> = unsafe { transmute(tx) };
    tx.send(Box::new(x)).unwrap();
}

fn unsafe_read_chan<A: marker::Send + 'static, E, T>
    (&Chan(_, ref rx, _): &Chan<E, T>) -> A
{
    let rx: &Receiver<Box<A>> = unsafe { transmute(rx) };
    *rx.recv().unwrap()
}

/// Peano numbers: Zero
#[allow(missing_copy_implementations)]
pub struct Z;

/// Peano numbers: Plus one
pub struct S<P> ( PhantomData<P> );

/// End of communication session (epsilon)
#[allow(missing_copy_implementations)]
pub struct Eps;

/// Receive `A`, then `R`
pub struct Recv<A,R> ( PhantomData<(A, R)> );

/// Send `A`, then `R`
pub struct Send<A,R> ( PhantomData<(A, R)> );

/// Active choice between `R` and `S`
pub struct Choose<R,S> ( PhantomData<(R, S)> );

/// Passive choice (offer) between `R` and `S`
pub struct Offer<R,S> ( PhantomData<(R, S)> );

/// Enter a recursive environment
pub struct Rec<R> ( PhantomData<R> );

/// Recurse. V indicates how many layers of the recursive environment we recurse
/// out of.
pub struct Var<V> ( PhantomData<V> );

/// Indicates that two protocols are dual
pub unsafe trait Dual: PhantomFn<Self> {}

unsafe impl Dual for (Eps, Eps) {}

unsafe impl <A, T, U> Dual for (Send<A, T>, Recv<A, U>)
    where (T, U): Dual {}

unsafe impl <A, T, U> Dual for (Recv<A, T>, Send<A, U>)
    where (T, U): Dual {}

unsafe impl<R, S, Rn, Sn> Dual for (Choose<R, Rn>, Offer<S, Sn>)
    where (R, S): Dual, (Rn, Sn): Dual {}

unsafe impl<R, S, Rn, Sn> Dual for (Offer<R, Rn>, Choose<S, Sn>)
    where (R, S): Dual, (Rn, Sn): Dual {}

unsafe impl Dual for (Var<Z>, Var<Z>) {}

unsafe impl<N> Dual for (Var<S<N>>, Var<S<N>>)
    where (Var<N>, Var<N>): Dual {}

unsafe impl<T, U> Dual for (Rec<T>, Rec<U>)
    where (T, U): Dual {}

/// Special duality check for environments
pub unsafe trait EnvDual: PhantomFn<Self> {}

unsafe impl EnvDual for ((), ()) {}

unsafe impl <R, R_, T, T_> EnvDual for ((R, T), (R_, T_))
    where (R, R_): Dual,
          (T, T_): EnvDual {}

impl<E> Chan<E, Eps> {
    /// Close a channel. Should always be used at the end of your program.
    pub fn close(self) {
        // Consume `c`
    }
}

impl<E, T, A: marker::Send + 'static> Chan<E, Send<A, T>> {
    /// Send a value of type `A` over the channel. Returns a channel with
    /// protocol `T`
    pub fn send(self, v: A) -> Chan<E, T> {
        unsafe_write_chan(&self, v);
        unsafe { transmute(self) }
    }
}

impl<E, T, A: marker::Send + 'static> Chan<E, Recv<A, T>> {
    /// Receives a value of type `A` from the channel. Returns a tuple
    /// containing the resulting channel and the received value.
    pub fn recv(self) -> (Chan<E, T>, A) {
        let v = unsafe_read_chan(&self);
        (unsafe { transmute(self) }, v)
    }
}

impl<E, R, S> Chan<E, Choose<R, S>> {
    /// Perform an active choice, selecting protocol `R`.
    pub fn sel1(self) -> Chan<E, R> {
        unsafe_write_chan(&self, true);
        unsafe { transmute(self) }
    }

    /// Perform an active choice, selecting protocol `S`.
    pub fn sel2(self) -> Chan<E, S> {
        unsafe_write_chan(&self, false);
        unsafe { transmute(self) }
    }
}

/// Convenience function. This is identical to `.sel2()`
impl<Z, A, B> Chan<Z, Choose<A, B>> {
    pub fn skip(self) -> Chan<Z, B> {
        self.sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2()`
impl<Z, A, B, C> Chan<Z, Choose<A, Choose<B, C>>> {
    pub fn skip2(self) -> Chan<Z, C> {
        self.sel2().sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2().sel2()`
impl<Z, A, B, C, D> Chan<Z, Choose<A, Choose<B, Choose<C, D>>>> {
    pub fn skip3(self) -> Chan<Z, D> {
        self.sel2().sel2().sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2().sel2()`
impl<Z, A, B, C, D, E> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D, E>>>>> {
    pub fn skip4(self) -> Chan<Z, E> {
        self.sel2().sel2().sel2().sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2().sel2().sel2()`
impl<Z, A, B, C, D, E, F> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D, Choose<E, F>>>>>> {
    pub fn skip5(self) -> Chan<Z, F> {
        self.sel2().sel2().sel2().sel2().sel2()
    }
}

/// Convenience function.
impl<Z, A, B, C, D, E, F, G> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D, Choose<E, Choose<F, G>>>>>>> {
    pub fn skip6(self) -> Chan<Z, G> {
        self.sel2().sel2().sel2().sel2().sel2().sel2()
    }
}

/// Convenience function.
impl<Z, A, B, C, D, E, F, G, H> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D, Choose<E, Choose<F, Choose<G, H>>>>>>>> {
    pub fn skip7(self) -> Chan<Z, H> {
        self.sel2().sel2().sel2().sel2().sel2().sel2().sel2()
    }
}

impl<E, R, S> Chan<E, Offer<R, S>> {
    /// Passive choice. This allows the other end of the channel to select one
    /// of two options for continuing the protocol: either `R` or `S`.
    pub fn offer(self) -> Result<Chan<E, R>, Chan<E, S>> {
        let b = unsafe_read_chan(&self);
        if b {
            Ok(unsafe { transmute(self) })
        } else {
            Err(unsafe { transmute(self) })
        }
    }
}

impl<E, R> Chan<E, Rec<R>> {
    /// Enter a recursive environment, putting the current environment on the
    /// top of the environment stack.
    pub fn enter(self) -> Chan<(R, E), R> {
        unsafe { transmute(self) }
    }
}

impl<E, R> Chan<(R, E), Var<Z>> {
    /// Recurse to the environment on the top of the environment stack.
    pub fn zero(self) -> Chan<(R, E), R> {
        unsafe { transmute(self) }
    }
}

impl<E, R, V> Chan<(R, E), Var<S<V>>> {
    /// Pop the top environment from the environment stack.
    pub fn succ(self) -> Chan<E, Var<V>> {
        unsafe { transmute(self) }
    }
}

/// Homogeneous select. We have a vector of channels, all obeying the same
/// protocol (and in the exact same point of the protocol), wait for one of them
/// to receive. Removes the receiving channel from the vector and returns both
/// the channel and the new vector.
pub fn hselect<E, P, A>(mut chans: Vec<Chan<E, Recv<A, P>>>)
                        -> (Chan<E, Recv<A, P>>, Vec<Chan<E, Recv<A, P>>>)
{
    let i = iselect(&chans);
    let c = chans.remove(i);
    (c, chans)
}

/// An alternative version of homogeneous select, returning the index of the Chan
/// that is ready to receive.
pub fn iselect<E, P, A>(chans: &Vec<Chan<E, Recv<A, P>>>) -> usize {
    let mut map = HashMap::new();

    let id = {
        let sel = Select::new();
        let mut handles = vec![]; // collect all the handles

        for (i, chan) in chans.iter().enumerate() {
            let &Chan(_, ref rx, _) = chan;
            let handle = sel.handle(rx);
            map.insert(handle.id(), i);
            handles.push(handle);
        }

        for handle in handles.iter_mut() { // Add
            unsafe { handle.add(); }
        }

        let id = sel.wait();

        for handle in handles.iter_mut() { // Clean up
            unsafe { handle.remove(); }
        }

        id
    };
    map.remove(&id).unwrap()
}

/// Heterogeneous selection structure for channels
///
/// This builds a structure of channels that we wish to select over. This is
/// structured in a way such that the channels selected over cannot be
/// interacted with (consumed) as long as the borrowing ChanSelect object
/// exists. This is necessary to ensure memory safety and should not pose an
///
/// The type parameter T is a return type, ie we store a value of some type T
/// that is returned in case its associated channels is selected on `wait()`
pub struct ChanSelect<'c, T> {
    chans: Vec<(&'c Chan<(), ()>, T)>,
}


impl<'c, T> ChanSelect<'c, T> {
    pub fn new() -> ChanSelect<'c, T> {
        ChanSelect {
            chans: Vec::new()
        }
    }

    /// Add a channel whose next step is Recv
    ///
    /// This method is marked unsafe, because of the lifetime transmute. If the
    /// Receiver goes out of scope nasty things can happen.
    pub fn add_ret<E, R, A: marker::Send>(&mut self,
                                          chan: &'c Chan<E, Recv<A, R>>,
                                          ret: T)
    {
        self.chans.push((unsafe { transmute(chan) }, ret));
    }

    /// Find a Receiver (and hence a Chan) that is ready to receive.
    ///
    /// This method consumes the ChanSelect, freeing up the borrowed Receivers
    /// to be consumed.
    pub fn wait(self) -> T {
        let sel = Select::new();
        let mut handles = Vec::with_capacity(self.chans.len());
        let mut map = HashMap::new();

        for (chan, ret) in self.chans.into_iter() {
            let &Chan(_, ref rx, _) = chan;
            let h = sel.handle(rx);
            let id = h.id();
            map.insert(id, ret);
            handles.push(h);
        }

        for handle in handles.iter_mut() {
            unsafe { handle.add(); }
        }

        let id = sel.wait();

        for handle in handles.iter_mut() {
            unsafe { handle.remove(); }
        }
        map.remove(&id).unwrap()
    }
}

impl<'c> ChanSelect<'c, usize> {
    pub fn add<E, R, A: marker::Send>(&mut self,
                                      c: &'c Chan<E, Recv<A, R>>)
    {
        let index = self.chans.len();
        self.add_ret(c, index);
    }
}

/// Sets up an session typed communication channel. Should be paired with
/// `request` for the corresponding client.
pub fn accept<E: marker::Send + 'static, R: marker::Send + 'static>
    (tx: Sender<Chan<E, R>>) -> Option<Chan<E, R>>
{
    borrow_accept(&tx)
}

pub fn borrow_accept<E: marker::Send + 'static, R: marker::Send + 'static>
    (tx: &Sender<Chan<E, R>>) -> Option<Chan<E, R>>
{
    let (tx1, rx1) = channel();
    let (tx2, rx2) = channel();

    let c1 = Chan (tx1, rx2, PhantomData);
    let c2 = Chan (tx2, rx1, PhantomData);

    match tx.send(c1) {
        Ok(_) => Some(c2),
        _ => None,
    }
}

/// Sets up an session typed communication channel. Should be paired with
/// `accept` for the corresponding server.
pub fn request<E: marker::Send + 'static, E_, R: marker::Send + 'static, S>
    (rx: Receiver<Chan<E, R>>) -> Option<Chan<E_, S>>
    where (R, S): Dual,
          (E, E_): EnvDual
{
    borrow_request(&rx)
}

pub fn borrow_request<E: marker::Send + 'static, E_, R: marker::Send + 'static, S>
    (rx: &Receiver<Chan<E, R>>) -> Option<Chan<E_, S>>
    where (R, S): Dual,
          (E, E_): EnvDual
{
    match rx.recv() {
        Ok(c) => Some(unsafe { transmute(c) }),
        _ => None,
    }
}

/// Returns two session channels
pub fn session_channel<E: marker::Send + 'static, E_, R: marker::Send + 'static, S>
    () -> (Chan<E, R>, Chan<E_, S>)
    where (R, S): Dual,
          (E, E_): EnvDual
{
    let (tx1, rx1) = channel();
    let (tx2, rx2) = channel();

    let c1 = Chan(tx1, rx2, PhantomData);
    let c2 = Chan(tx2, rx1, PhantomData);

    (c1, c2)
}

/// Connect two functions using a session typed channel.
pub fn connect<E: marker::Send + 'static, E_, F1, F2, R: marker::Send + 'static, S>
    (srv: F1, cli: F2)
    where F1: Fn(Chan<E, R>) + marker::Send,
          F2: Fn(Chan<E_, S>) + marker::Send,
          (R, S): Dual,
          (E, E_): EnvDual
{
    let (tx, rx) = channel();
    let joinguard = scoped(move || srv(accept(tx).unwrap()));
    cli(request(rx).unwrap());
    joinguard.join();
}



#[macro_export]
macro_rules! offer {
    (
        $id:ident, $branch:ident => $code:expr, $($t:tt)+
    ) => (
        match $id.offer() {
            Ok($id) => $code,
            Err($id) => offer!{ $id, $($t)+ }
        }
    );
    (
        $id:ident, $branch:ident => $code:expr
    ) => (
        $code
    )
}

#[macro_export]
macro_rules! chan_select {
    (
        $(($c:ident, $name:pat) = $rx:ident.recv() => $code:expr),+
    ) => ({
        let index = {
            let mut sel = $crate::ChanSelect::new();
            $( sel.add(&$rx); )+
            sel.wait()
        };

        let mut i: usize = 0;

        $( if index == { i += 1; i - 1 } { let ($c, $name) = $rx.recv(); $code } else )+
        { unreachable!() }
    })
}