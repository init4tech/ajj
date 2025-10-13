use bytes::{Buf, Bytes, BytesMut};
use futures_util::Stream;
use interprocess::local_socket::{
    tokio::{Listener, RecvHalf, SendHalf},
    traits::tokio::Stream as _,
    ListenerOptions,
};
use pin_project::pin_project;
use serde_json::value::RawValue;
use std::{
    io,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio_util::io::poll_read_buf;
use tracing::{debug, error, trace};

impl crate::pubsub::Listener for Listener {
    type RespSink = SendHalf;

    type ReqStream = IpcBytesStream;

    type Error = io::Error;

    async fn accept(&self) -> Result<(Self::RespSink, Self::ReqStream), Self::Error> {
        let conn = interprocess::local_socket::traits::tokio::Listener::accept(self).await?;

        let (recv, send) = conn.split();

        Ok((send, recv.into()))
    }
}

impl crate::pubsub::JsonSink for SendHalf {
    type Error = std::io::Error;

    async fn send_json(&mut self, json: Box<RawValue>) -> Result<(), Self::Error> {
        self.write_all(json.get().as_bytes()).await
    }
}

impl crate::pubsub::Connect for ListenerOptions<'_> {
    type Listener = Listener;

    type Error = io::Error;

    async fn make_listener(self) -> Result<Self::Listener, Self::Error> {
        self.create_tokio()
    }
}

/// A stream that pulls data from the IPC connection and yields [`Bytes`]
/// containing
#[derive(Debug)]
#[pin_project]
pub struct IpcBytesStream {
    #[pin]
    inner: ReadJsonStream<RecvHalf, Box<RawValue>>,

    /// Whether the stream has been drained.
    drained: bool,
}

impl IpcBytesStream {
    fn new(inner: RecvHalf) -> Self {
        Self {
            inner: inner.into(),
            drained: false,
        }
    }
}

impl From<RecvHalf> for IpcBytesStream {
    fn from(inner: RecvHalf) -> Self {
        Self::new(inner)
    }
}

impl Stream for IpcBytesStream {
    type Item = Bytes;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match ready!(this.inner.poll_next(cx)) {
            Some(item) => {
                let item: Box<str> = item.into();
                Poll::Ready(Some(Bytes::from_owner(item.into_boxed_bytes())))
            }
            None => Poll::Ready(None),
        }
    }
}

/// Default capacity for the IPC buffer.
const CAPACITY: usize = 4096;

/// A stream of JSON-RPC items, read from an [`AsyncRead`] stream.
#[derive(Debug)]
#[pin_project::pin_project]
#[doc(hidden)]
pub struct ReadJsonStream<T, Item> {
    /// The underlying reader.
    #[pin]
    reader: T,
    /// A buffer for reading data from the reader.
    buf: BytesMut,
    /// Whether the buffer has been drained.
    drained: bool,

    /// PhantomData marking the item type this stream will yield.
    _pd: std::marker::PhantomData<Item>,
}

impl<T: AsyncRead, U> ReadJsonStream<T, U> {
    fn new(reader: T) -> Self {
        Self {
            reader,
            buf: BytesMut::with_capacity(CAPACITY),
            drained: true,
            _pd: core::marker::PhantomData,
        }
    }
}

impl<T: AsyncRead, U> From<T> for ReadJsonStream<T, U> {
    fn from(reader: T) -> Self {
        Self::new(reader)
    }
}

impl<T: AsyncRead, Item> Stream for ReadJsonStream<T, Item>
where
    Item: serde::de::DeserializeOwned,
{
    type Item = Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            // try decoding from the buffer, but only if we have new data
            if !*this.drained {
                debug!(buf_len = this.buf.len(), "Deserializing buffered IPC data");
                let mut de = serde_json::Deserializer::from_slice(this.buf.as_ref()).into_iter();

                let item = de.next();

                // advance the buffer
                this.buf.advance(de.byte_offset());

                match item {
                    Some(Ok(response)) => {
                        return Poll::Ready(Some(response));
                    }
                    Some(Err(err)) => {
                        if err.is_data() {
                            trace!(
                                buffer = %String::from_utf8_lossy(this.buf.as_ref()),
                                "IPC buffer contains invalid JSON data",
                            );

                            // this happens if the deserializer is unable to decode a partial object
                            *this.drained = true;
                        } else if err.is_eof() {
                            trace!("partial object in IPC buffer");
                            // nothing decoded
                            *this.drained = true;
                        } else {
                            error!(%err, "IPC response contained invalid JSON. Buffer contents will be logged at trace level");
                            trace!(
                                buffer = %String::from_utf8_lossy(this.buf.as_ref()),
                                "IPC response contained invalid JSON. NOTE: Buffer contents do not include invalid utf8.",
                            );

                            return Poll::Ready(None);
                        }
                    }
                    None => {
                        // nothing decoded
                        *this.drained = true;
                    }
                }
            }

            // read more data into the buffer
            match ready!(poll_read_buf(this.reader.as_mut(), cx, &mut this.buf)) {
                Ok(0) => {
                    // stream is no longer readable and we're also unable to decode any more
                    // data. This happens if the IPC socket is closed by the other end.
                    // so we can return `None` here.
                    debug!("IPC socket EOF, stream is closed");
                    return Poll::Ready(None);
                }
                Ok(data_len) => {
                    debug!(%data_len, "Read data from IPC socket");
                    // can try decoding again
                    *this.drained = false;
                }
                Err(err) => {
                    error!(%err, "Failed to read from IPC socket, shutting down");
                    return Poll::Ready(None);
                }
            }
        }
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `alloy` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/alloy-rs/alloy
//
// The MIT License (MIT)
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.
