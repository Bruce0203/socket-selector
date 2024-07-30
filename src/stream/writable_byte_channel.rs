use fast_collections::Cursor;

use super::{
    write_registry::{RegisterWrite, SelectorWriteRegistry},
    Accept, Close, Flush, Open, Read, ReadError, Write,
};

pub struct WritableByteChannel<T, const LEN: usize> {
    stream: T,
    pub write_buf: Cursor<u8, LEN>,
}

impl<T, const LEN: usize> From<T> for WritableByteChannel<T, LEN> {
    fn from(value: T) -> Self {
        Self {
            stream: value,
            write_buf: Cursor::new(),
        }
    }
}

impl<T: Accept<A>, A, const LEN: usize> Accept<A> for WritableByteChannel<T, LEN> {
    fn accept(accept: A) -> Self {
        Self {
            stream: T::accept(accept),
            write_buf: Cursor::default(),
        }
    }

    fn get_stream(&mut self) -> &mut A {
        self.stream.get_stream()
    }
}

impl<T: Read, const LEN: usize> Read for WritableByteChannel<T, LEN> {
    fn read<const N: usize>(&mut self, read_buf: &mut Cursor<u8, N>) -> Result<(), ReadError> {
        self.stream.read(read_buf)
    }
}

impl<T: Write, const LEN: usize> Write for WritableByteChannel<T, LEN> {
    fn write<const N: usize>(&mut self, write_buf: &mut Cursor<u8, N>) -> Result<(), Self::Error> {
        self.stream.write(write_buf)
    }
}

impl<T: Flush + Write, const LEN: usize> Flush for WritableByteChannel<T, LEN> {
    type Error = <T as Flush>::Error;
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.stream.write(&mut self.write_buf)?;
        self.stream.flush()
    }
}

impl<T: Close, const LEN: usize> Close for WritableByteChannel<T, LEN> {
    type Error = T::Error;

    type Registry = T::Registry;

    fn close(&mut self, registry: &mut Self::Registry) -> Result<(), Self::Error> {
        self.stream.close(registry)
    }

    fn is_closed(&self) -> bool {
        self.stream.is_closed()
    }
}

impl<T: Open, const LEN: usize> Open for WritableByteChannel<T, LEN> {
    type Error = T::Error;

    type Registry = T::Registry;

    fn open(&mut self, registry: &mut Self::Registry) -> Result<(), Self::Error> {
        self.stream.open(registry)
    }
}

impl<T: RegisterWrite<P, LEN>, P, const LEN: usize> RegisterWrite<P, LEN>
    for WritableByteChannel<T, LEN>
{
    fn get_selector_registry_mut(&mut self) -> &mut SelectorWriteRegistry<P, LEN> {
        self.stream.get_selector_registry_mut()
    }
}
