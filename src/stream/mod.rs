use std::{fmt::Debug, marker::PhantomData};

use fast_collections::Cursor;
use nonmax::NonMaxUsize;

pub mod write_registry;
pub mod mio;
pub mod mock;
pub mod packet;
pub mod readable_byte_channel;
pub mod websocket;
pub mod writable_byte_channel;

pub trait Read {
    fn read<const N: usize>(&mut self, read_buf: &mut Cursor<u8, N>) -> Result<(), ReadError>;
}

#[derive(Debug)]
pub enum ReadError {
    NotFullRead,
    FlushRequest,
    SocketClosed,
}

pub trait Write: Flush {
    fn write<const N: usize>(&mut self, write: &mut Cursor<u8, N>) -> Result<(), Self::Error>;
}

pub trait Flush {
    type Error;
    fn flush(&mut self) -> Result<(), Self::Error>;
}

pub trait Close {
    type Error;
    type Registry;
    fn is_closed(&self) -> bool;
    fn close(&mut self, registry: &mut Self::Registry) -> Result<(), Self::Error>;
}

pub trait Open {
    type Error;
    type Registry;
    fn open(&mut self, registry: &mut Self::Registry) -> Result<(), Self::Error>;
}

pub trait Accept<T>: Sized {
    fn get_stream(&mut self) -> &mut T;
    fn accept(accept: T) -> Self;
}

#[repr(C)]
pub struct Id<T> {
    inner: NonMaxUsize,
    _marker: PhantomData<T>,
}

const _: () = {
    if size_of::<Id<()>>() != size_of::<usize>() {
        panic!("size of id is not same as usize")
    }
};

impl<T> Copy for Id<T> {}

impl<T> Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Id").field("id", &self.inner).finish()
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> Into<usize> for &Id<T> {
    fn into(self) -> usize {
        self.inner.get()
    }
}

impl<T> Into<usize> for Id<T> {
    fn into(self) -> usize {
        self.inner.get()
    }
}

impl<T> Into<NonMaxUsize> for Id<T> {
    fn into(self) -> NonMaxUsize {
        self.inner
    }
}
impl<T> Into<NonMaxUsize> for &Id<T> {
    fn into(self) -> NonMaxUsize {
        self.inner
    }
}

impl<T> Id<T> {
    pub const unsafe fn from(value: usize) -> Self {
        Self {
            inner: unsafe { NonMaxUsize::new_unchecked(value) },
            _marker: PhantomData,
        }
    }
}
