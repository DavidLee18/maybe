use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::atomic::Ordering;
use std::{self, io};

use super::super::{add_socket, co_io_result, IoData};
use crate::coroutine_impl::{co_cancel_data, CoroutineImpl, EventSource};
use crate::io::AsIoData;
use crate::net::{TcpListener, TcpStream};
use crate::sync::delay_drop::DelayDrop;
use crate::yield_now::yield_with;

pub struct TcpListenerAccept<'a> {
    io_data: &'a IoData,
    socket: &'a std::net::TcpListener,
    can_drop: DelayDrop,
}

impl<'a> TcpListenerAccept<'a> {
    pub fn new(socket: &'a TcpListener) -> io::Result<Self> {
        Ok(TcpListenerAccept {
            io_data: socket.as_io_data(),
            socket: socket.inner(),
            can_drop: DelayDrop::new(),
        })
    }

    #[inline]
    pub fn done(self) -> io::Result<(TcpStream, SocketAddr)> {
        loop {
            co_io_result()?;

            // clear the io_flag
            self.io_data.io_flag.store(false, Ordering::Relaxed);

            match self.socket.accept() {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                ret => {
                    return ret.and_then(|(s, a)| {
                        s.set_nonblocking(true)?;
                        add_socket(&s).map(|io| (TcpStream::from_stream(s, io), a))
                    });
                }
            }

            if self.io_data.io_flag.swap(false, Ordering::Relaxed) {
                continue;
            }

            // the result is still WouldBlock, need to try again
            self.can_drop.reset();
            yield_with(&self);
        }
    }
}

impl<'a> EventSource for TcpListenerAccept<'a> {
    fn subscribe(&mut self, co: CoroutineImpl) {
        let _g = self.can_drop.delay_drop();
        let cancel = co_cancel_data(&co);
        // if there is no timer we don't need to call add_io_timer
        self.io_data.co.swap(co, Ordering::Release);

        // there is event happened
        if self.io_data.io_flag.load(Ordering::Acquire) {
            return self.io_data.schedule();
        }

        // register the cancel io data
        cancel.set_io(self.io_data.deref().clone());
        // re-check the cancel status
        if cancel.is_canceled() {
            unsafe { cancel.cancel() };
        }
    }
}
