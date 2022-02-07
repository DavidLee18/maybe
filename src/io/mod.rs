//! coroutine io utilities

#[cfg(unix)]
#[path = "sys/unix/mod.rs"]
pub(crate) mod sys;

#[cfg(windows)]
#[path = "sys/windows/mod.rs"]
pub(crate) mod sys;

// export the generic IO wrapper
pub mod co_io_err;

mod event_loop;

use std::io;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::coroutine_impl::is_coroutine;

pub(crate) use self::event_loop::EventLoop;
pub use self::sys::co_io::CoIo;
#[cfg(unix)]
pub use self::sys::wait_io::WaitIo;
pub(crate) use self::sys::{add_socket, cancel, net, IoData, Selector};

pub trait AsIoData {
    fn as_io_data(&self) -> &IoData;
}

#[derive(Debug)]
pub(crate) struct IoContext {
    nonblocking: AtomicBool,
    blocked_io: AtomicBool,
}

impl IoContext {
    pub fn new() -> Self {
        IoContext {
            // default is blocking mode in coroutine context
            nonblocking: AtomicBool::new(false),
            // track current io object nonblocking mode
            blocked_io: AtomicBool::new(true),
        }
    }

    // return Ok(true) if it's a nonblocking request
    // f is a closure to set the actual inner io nonblocking mode
    #[inline]
    pub fn check_nonblocking<F>(&self, f: F) -> io::Result<bool>
    where
        F: FnOnce(bool) -> io::Result<()>,
    {
        // for coroutine context
        if self.nonblocking.load(Ordering::Relaxed) {
            {
                if !self.blocked_io.load(Ordering::Relaxed) {
                    f(true)?;
                    self.blocked_io.store(true, Ordering::Relaxed);
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    // return Ok(true) if it's a coroutine context
    // f is a closure to set the actual inner io nonblocking mode
    #[inline]
    pub fn check_context<F>(&self, f: F) -> io::Result<bool>
    where
        F: FnOnce(bool) -> io::Result<()>,
    {
        // thread context
        if !is_coroutine() {
            {
                if self.blocked_io.load(Ordering::Relaxed) {
                    f(false)?;
                    self.blocked_io.store(false, Ordering::Relaxed);
                }
                return Ok(false);
            }
        }

        // for coroutine context
        if !self.blocked_io.load(Ordering::Relaxed) {
            f(true)?;
            self.blocked_io.store(true, Ordering::Relaxed);
        }
        Ok(true)
    }

    #[inline]
    pub fn set_nonblocking(&self, nonblocking: bool) {
        self.nonblocking.store(nonblocking, Ordering::Relaxed);
    }
}

// an option type that implement deref
struct OptionCell<T>(Option<T>);

impl<T> Deref for OptionCell<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self.0.as_ref() {
            Some(d) => d,
            None => panic!("no data to deref for OptionCell"),
        }
    }
}

impl<T> OptionCell<T> {
    pub fn new(data: T) -> Self {
        OptionCell(Some(data))
    }

    pub fn take(&mut self) -> T {
        match self.0.take() {
            Some(d) => d,
            None => panic!("no data to take for OptionCell"),
        }
    }
}
