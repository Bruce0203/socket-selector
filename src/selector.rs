use fast_collections::{Clear, Cursor, GetUnchecked, Push, Slab, Vec};

use super::stream::{Accept, Close, Flush, Id, Open, Read, ReadError, Write};
use crate::{
    connection::ConnectionPipe,
    stream::{packet::WritePacket, readable_byte_channel::PollRead},
};

// 1. mock testing: could be toggled in feature
// 2. rw buf size: could be custom allocator(maybe not sure)
// 3. max connection size: could be custom allocator(maybe not sure)
// 4. socket type(could be stuck in system, so it should be ignored)
// 5. multithread selector(not related)

// 기존 프로젝트의 문제점
// 1. 불필요한 제네릭이 수없이 많이 전파되어 코드가 더러워진다
//   - 해결방법:
//     * mock 테스팅: feature 로 토글링 가능하다.
//     * 읽고쓰는 버퍼 크기:  커스텀 할당기로 대체한다.
//     *  최대 인원 크기:  커스텀 할당기로 대체한다.
//     * 소켓 타입:  굳이 소켓을 제네릭화할 필요성을 못느낌.
//
// 앞으로 추가할 기능
//
// 쓰기 이밴트 등록기를 LCell이나 아예 커스텀 포인터를 통해 만든다.
// 1. create write event registry (with lcell maybe)
// 2. create socket library with togglable feature that support mock testing
// 3. create custom allocator

#[derive(derive_more::Deref, derive_more::DerefMut)]
pub struct Selector<T, S, C, const N: usize> {
    #[deref]
    #[deref_mut]
    server: T,
    pub(crate) connections: Slab<Id<C>, SelectableChannel<ConnectionPipe<S, C>>, N>,
    write_or_close_events: Vec<Id<C>, N>,
}

impl<T, S, C, const N: usize> Selector<T, S, C, N> {
    pub fn get(&self, id: &Id<C>) -> &SelectableChannel<ConnectionPipe<S, C>> {
        unsafe { self.connections.get_unchecked(id) }
    }

    pub fn get_mut(&mut self, id: &Id<C>) -> &mut SelectableChannel<ConnectionPipe<S, C>> {
        unsafe { self.connections.get_unchecked_mut(id) }
    }
}

impl<T, S, C, const N: usize> From<T> for Selector<T, S, C, N> {
    fn from(value: T) -> Self {
        Self {
            server: value,
            connections: Slab::new(),
            write_or_close_events: Vec::uninit(),
        }
    }
}

impl<T: Default, S, C, const N: usize> Default for Selector<T, S, C, N> {
    fn default() -> Self {
        Self {
            server: Default::default(),
            connections: Default::default(),
            write_or_close_events: Default::default(),
        }
    }
}

impl<T, S, C, const N: usize> Selector<T, S, C, N> {
    pub fn new(server: T) -> Self {
        Self {
            server,
            connections: Slab::new(),
            write_or_close_events: Vec::uninit(),
        }
    }
}

pub trait SelectorListener<T, C, const N: usize>: Sized {
    fn tick(server: &mut Selector<Self, T, C, N>) -> Result<(), ()>;
    fn accept(server: &mut Selector<Self, T, C, N>, id: Id<C>);
    fn read(server: &mut Selector<Self, T, C, N>, id: Id<C>) -> Result<(), ReadError>;
    fn close(server: &mut Selector<Self, T, C, N>, id: Id<C>);
}

#[derive(derive_more::Deref, derive_more::DerefMut)]
pub struct SelectableChannel<T> {
    #[deref]
    #[deref_mut]
    pub stream: T,
    read_failure_accumulator: u8,
    state: SelectorState,
}

impl<T: Accept<A>, A> Accept<A> for SelectableChannel<T> {
    fn accept(accept: A) -> Self {
        Self {
            read_failure_accumulator: 0,
            stream: T::accept(accept),
            state: SelectorState::default(),
        }
    }

    fn get_stream(&mut self) -> &mut A {
        self.stream.get_stream()
    }
}

impl<T: Open> Open for SelectableChannel<T> {
    type Error = T::Error;
    type Registry = T::Registry;

    fn open(&mut self, registry: &mut Self::Registry) -> Result<(), Self::Error> {
        self.stream.open(registry)
    }
}

impl<T: Close> Close for SelectableChannel<T> {
    type Error = T::Error;

    type Registry = T::Registry;

    fn is_closed(&self) -> bool {
        self.stream.is_closed()
    }

    fn close(&mut self, registry: &mut Self::Registry) -> Result<(), Self::Error> {
        self.stream.close(registry)
    }
}

impl<T: Write> Write for SelectableChannel<T> {
    fn write<const N: usize>(&mut self, write: &mut Cursor<u8, N>) -> Result<(), Self::Error> {
        self.stream.write(write)
    }
}

impl<T: Flush> Flush for SelectableChannel<T> {
    type Error = T::Error;
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.stream.flush()
    }
}

impl<T: Read> Read for SelectableChannel<T> {
    fn read<const N: usize>(&mut self, read_buf: &mut Cursor<u8, N>) -> Result<(), ReadError> {
        self.stream.read(read_buf)
    }
}

#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SelectorState {
    #[default]
    Idle,
    CloseRequested,
    FlushRequested,
}

impl<T: SelectorListener<S, C, N>, C, S: Close + Flush + PollRead, const N: usize>
    Selector<T, S, C, N>
{
    pub fn request_socket_close(&mut self, id: Id<C>) {
        let socket = self.get_mut(&id);
        socket.state = SelectorState::CloseRequested;
        self.write_or_close_events.push(id).map_err(|_| ()).unwrap();
    }

    pub fn request_socket_flush(&mut self, id: Id<C>) {
        let socket = self.get_mut(&id);
        if socket.state == SelectorState::Idle {
            socket.state = SelectorState::FlushRequested;
            self.write_or_close_events.push(id).map_err(|_| ()).unwrap();
        }
    }

    pub fn handle_read(&mut self, id: Id<C>, _registry: &mut <S as Close>::Registry) {
        let mut f = || -> Result<(), ReadError> {
            let socket = self.get_mut(&id);
            socket.poll_read()?;
            T::read(self, id.clone())?;
            Ok(())
        };
        match f() {
            Ok(()) => {
                let socket = self.get_mut(&id);
                socket.read_failure_accumulator = 0;
            }
            Err(err) => match err {
                ReadError::NotFullRead => {
                    let socket = self.get_mut(&id);
                    socket.read_failure_accumulator += 1;
                    if socket.read_failure_accumulator == u8::MAX {
                        socket.read_failure_accumulator = 0;
                        self.request_socket_close(id.clone());
                    }
                }
                ReadError::SocketClosed => self.request_socket_close(id.clone()),
                ReadError::FlushRequest => self.request_socket_flush(id.clone()),
            },
        };
    }

    pub(crate) fn flush_all_sockets(&mut self, registry: &mut <S as Close>::Registry) {
        let len = self.write_or_close_events.len();
        let mut index = 0;
        while index < len {
            let socket_id = unsafe { self.write_or_close_events.get_unchecked(index) }.clone();
            let socket = unsafe { self.connections.get_unchecked_mut(&socket_id) };
            index += 1;
            match socket.state {
                SelectorState::Idle => continue,
                SelectorState::CloseRequested => {
                    T::close(self, socket_id.clone());
                    let socket = self.get_mut(&socket_id);
                    let _result = socket.stream.close(registry);
                }
                SelectorState::FlushRequested => {
                    socket.state = SelectorState::Idle;
                    if socket.stream.flush().is_err() {
                        self.request_socket_close(socket_id);
                    }
                }
            }
        }
        self.write_or_close_events.clear();
    }
}

impl<P, T: WritePacket<P>> WritePacket<P> for SelectableChannel<T> {
    fn send(&mut self, packet: P) -> Result<(), ReadError> {
        self.stream.send(packet)
    }
}

impl<T: PollRead> PollRead for SelectableChannel<T> {
    fn poll_read(&mut self) -> Result<(), ReadError> {
        self.stream.poll_read()
    }
}
