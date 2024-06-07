use std::future::Future;
use std::io::ErrorKind;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::{TcpStream, UnixStream};
use tokio::sync::Mutex;

use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::racoon_debug;

pub type StreamResult<T> = Box<dyn Future<Output = T> + Sync + Send + Unpin>;
pub type Stream = Box<dyn AbstractStream>;

pub trait AbstractStream: Sync + Send {
    fn buffer_size(&self) -> StreamResult<usize>;
    fn peer_addr(&self) -> StreamResult<Option<String>>;
    fn restore_payload(&self, bytes: &[u8]) -> StreamResult<std::io::Result<()>>;
    fn restored_len(&self) -> StreamResult<usize>;
    fn read_chunk(&self) -> StreamResult<std::io::Result<Vec<u8>>>;
    fn write_chunk(&self, bytes: &[u8]) -> StreamResult<std::io::Result<()>>;
    fn shutdown(&self) -> StreamResult<std::io::Result<()>>;
}

#[derive(Debug)]
pub struct TcpStreamWrapper {
    stream: Arc<Mutex<TcpStream>>,
    reader: Arc<Mutex<ReadHalf<TcpStream>>>,
    writer: Arc<Mutex<WriteHalf<TcpStream>>>,
    buffer_size: usize,
    restored_payload: Arc<Mutex<Option<Vec<u8>>>>,
}

impl TcpStreamWrapper {
    pub fn from(tcp_stream: TcpStream, buffer_size: usize) -> std::io::Result<Self> {
        let std_tcp_stream = tcp_stream.into_std()?.try_clone()?;
        // Stream for shutting down later
        let async_tcp_stream = TcpStream::from_std(std_tcp_stream.try_clone()?)?;
        // Stream for reader/writer
        let async_tcp_stream_rw = TcpStream::from_std(std_tcp_stream.try_clone()?)?;
        let (reader, writer) = tokio::io::split(async_tcp_stream_rw);

        Ok(Self {
            stream: Arc::new(Mutex::new(async_tcp_stream)),
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            buffer_size,
            restored_payload: Arc::new(Mutex::new(None)),
        })
    }
}

impl AbstractStream for TcpStreamWrapper {
    fn buffer_size(&self) -> StreamResult<usize> {
        let buffer_size = self.buffer_size.clone();
        Box::new(Box::pin(async move { buffer_size }))
    }

    fn peer_addr(&self) -> StreamResult<Option<String>> {
        let stream_ref = self.stream.clone();

        Box::new(Box::pin(async move {
            let stream = stream_ref.lock().await;

            return match stream.peer_addr() {
                Ok(addr) => Some(addr.to_string()),
                Err(error) => {
                    racoon_debug!("Failed to get peer addr. Error: {}", error);
                    None
                }
            };
        }))
    }

    fn restore_payload(&self, bytes: &[u8]) -> StreamResult<std::io::Result<()>> {
        let restored_payload_ref = self.restored_payload.clone();
        let bytes = bytes.to_vec();

        Box::new(Box::pin(async move {
            let mut restored_payload = restored_payload_ref.lock().await;
            *restored_payload = Some(bytes.to_vec());
            Ok(())
        }))
    }

    fn restored_len(&self) -> StreamResult<usize> {
        let restored_payload_ref = self.restored_payload.clone();

        Box::new(Box::pin(async move {
            let restored_payload = restored_payload_ref.lock().await;

            if let Some(restored) = restored_payload.as_ref() {
                return restored.len();
            }

            0
        }))
    }

    fn read_chunk(&self) -> StreamResult<std::io::Result<Vec<u8>>> {
        let restored_payload_ref = self.restored_payload.clone();
        let reader_ref = self.reader.clone();
        let buffer_size = self.buffer_size.clone();

        Box::new(Box::pin(async move {
            // If payload of some bytes is restored after reading the chunk, returns the same bytes
            // back to the reader again.
            // Reading from stream wrapper is skipped because there may not be any bytes to read.
            let mut restored_payload = restored_payload_ref.lock().await;

            if let Some(payload) = restored_payload.as_ref() {
                let buffer = payload.to_owned();
                *restored_payload = None;
                return Ok(buffer);
            }

            let mut buffer = vec![0u8; buffer_size];
            let mut reader = reader_ref.lock().await;

            return match reader.read(&mut buffer).await {
                Ok(read_size) => {
                    if read_size == 0 {
                        return Err(std::io::Error::new(
                            ErrorKind::BrokenPipe,
                            "Read size is 0. Probably connection broken.",
                        ));
                    }

                    let chunk = &buffer[0..read_size];
                    Ok(chunk.to_vec())
                }
                Err(error) => Err(std::io::Error::other(error)),
            };
        }))
    }

    fn write_chunk(&self, data: &[u8]) -> StreamResult<std::io::Result<()>> {
        let writer_ref = self.writer.clone();
        let data = data.to_vec().clone();

        Box::new(Box::pin(async move {
            let mut writer = writer_ref.lock().await;
            writer.write_all(&data).await?;
            Ok(())
        }))
    }

    fn shutdown(&self) -> StreamResult<std::io::Result<()>> {
        let stream_ref = self.stream.clone();

        Box::new(Box::pin(async move {
            let mut stream = stream_ref.lock().await;
            let _ = stream.shutdown().await;
            Ok(())
        }))
    }
}

#[derive(Debug)]
pub struct UnixStreamWrapper {
    stream: Arc<Mutex<UnixStream>>,
    reader: Arc<Mutex<ReadHalf<UnixStream>>>,
    writer: Arc<Mutex<WriteHalf<UnixStream>>>,
    buffer_size: usize,
    restored_payload: Arc<Mutex<Option<Vec<u8>>>>,
}

impl UnixStreamWrapper {
    pub fn from(unix_stream: UnixStream, buffer_size: usize) -> std::io::Result<Self> {
        let std_unix_stream = unix_stream.into_std()?;
        let async_unix_stream = UnixStream::from_std(std_unix_stream.try_clone()?)?;
        let async_writer_rw = UnixStream::from_std(std_unix_stream)?;
        let (reader, writer) = tokio::io::split(async_writer_rw);

        Ok(Self {
            stream: Arc::new(Mutex::new(async_unix_stream)),
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            buffer_size,
            restored_payload: Arc::new(Mutex::new(None)),
        })
    }
}

impl AbstractStream for UnixStreamWrapper {
    fn buffer_size(&self) -> StreamResult<usize> {
        let buffer_size = self.buffer_size.clone();
        Box::new(Box::pin(async move { buffer_size }))
    }

    fn peer_addr(&self) -> StreamResult<Option<String>> {
        Box::new(Box::pin(async move {
            return None;
        }))
    }

    fn restore_payload(&self, bytes: &[u8]) -> StreamResult<std::io::Result<()>> {
        let restored_payload = self.restored_payload.clone();
        let bytes = bytes.to_vec();

        Box::new(Box::pin(async move {
            let restored_payload_ref = restored_payload.clone();
            let mut restored_payload = restored_payload_ref.lock().await;
            *restored_payload = Some(bytes);
            Ok(())
        }))
    }

    fn restored_len(&self) -> StreamResult<usize> {
        let restored_payload = self.restored_payload.clone();

        Box::new(Box::pin(async move {
            let restored_payload_ref = restored_payload.clone();
            let restored_payload = restored_payload_ref.lock().await;

            if let Some(restored) = restored_payload.as_ref() {
                return restored.len();
            }

            0
        }))
    }

    fn read_chunk(&self) -> StreamResult<std::io::Result<Vec<u8>>> {
        // If payload of some bytes is restored after reading the chunk, returns the same bytes
        // back to the reader again.
        // Reading from stream wrapper is skipped because there may not be any bytes to read.
        let restored_payload_ref = self.restored_payload.clone();
        let buffer_size = self.buffer_size.clone();

        let reader = self.reader.clone();

        Box::new(Box::pin(async move {
            let mut restored_payload = restored_payload_ref.lock().await;

            if let Some(payload) = restored_payload.as_ref() {
                let buffer = payload.to_owned();
                *restored_payload = None;
                return Ok(buffer);
            }

            let mut buffer = vec![0u8; buffer_size];

            let reader_ref = reader.clone();
            let mut reader = reader_ref.lock().await;

            return match reader.read(&mut buffer).await {
                Ok(read_size) => {
                    if read_size == 0 {
                        return Err(std::io::Error::new(
                            ErrorKind::BrokenPipe,
                            "Read size is 0. Probably connection broken.",
                        ));
                    }

                    let chunk = &buffer[0..read_size];
                    Ok(chunk.to_vec())
                }
                Err(error) => Err(std::io::Error::other(error)),
            };
        }))
    }

    fn write_chunk(&self, data: &[u8]) -> StreamResult<std::io::Result<()>> {
        let writer_ref = self.writer.clone();
        let data = data.to_vec();

        Box::new(Box::pin(async move {
            let mut writer = writer_ref.lock().await;
            writer.write_all(&data).await?;
            Ok(())
        }))
    }

    fn shutdown(&self) -> StreamResult<std::io::Result<()>> {
        let stream_ref = self.stream.clone();

        Box::new(Box::pin(async move {
            let mut stream = stream_ref.lock().await;
            let _ = stream.shutdown().await;
            Ok(())
        }))
    }
}

#[derive(Debug)]
pub struct TlsTcpStreamWrapper {
    peer_addr: String,
    stream: Arc<Mutex<TcpStream>>,
    reader: Arc<Mutex<ReadHalf<TlsStream<TcpStream>>>>,
    writer: Arc<Mutex<WriteHalf<TlsStream<TcpStream>>>>,
    buffer_size: usize,
    restored_payload: Arc<Mutex<Option<Vec<u8>>>>,
}

impl TlsTcpStreamWrapper {
    pub async fn from(
        tcp_stream: TcpStream,
        tls_acceptor: &TlsAcceptor,
        buffer_size: usize,
    ) -> std::io::Result<Self> {
        let peer_addr = tcp_stream.peer_addr()?.to_string();
        let std_tcp_stream = tcp_stream.into_std()?;

        // Stream for shutting down reader/writer later
        let stream = TcpStream::from_std(std_tcp_stream.try_clone()?)?;
        let async_reader = TcpStream::from_std(std_tcp_stream)?;

        let tls_async_stream = tls_acceptor.accept(async_reader).await?;
        let (reader, writer) = tokio::io::split(tls_async_stream);

        Ok(Self {
            peer_addr,
            stream: Arc::new(Mutex::new(stream)),
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            buffer_size,
            restored_payload: Arc::new(Mutex::new(None)),
        })
    }
}

impl AbstractStream for TlsTcpStreamWrapper {
    fn buffer_size(&self) -> StreamResult<usize> {
        let buffer_size = self.buffer_size.clone();
        Box::new(Box::pin(async move { buffer_size }))
    }

    fn peer_addr(&self) -> StreamResult<Option<String>> {
        let peer_addr = self.peer_addr.clone();

        Box::new(Box::pin(async move { Some(peer_addr) }))
    }

    fn restore_payload(&self, bytes: &[u8]) -> StreamResult<std::io::Result<()>> {
        let restored_payload_ref = self.restored_payload.clone();

        let bytes = bytes.to_vec();

        Box::new(Box::pin(async move {
            let mut restored_payload = restored_payload_ref.lock().await;
            *restored_payload = Some(bytes);
            Ok(())
        }))
    }

    fn restored_len(&self) -> StreamResult<usize> {
        let restored_payload_ref = self.restored_payload.clone();

        Box::new(Box::pin(async move {
            let restored_payload = restored_payload_ref.lock().await;

            if let Some(restored) = restored_payload.as_ref() {
                return restored.len();
            }

            0
        }))
    }

    fn read_chunk(&self) -> StreamResult<std::io::Result<Vec<u8>>> {
        // If payload of some bytes is restored after reading the chunk, returns the same bytes
        // back to the reader again.
        // Reading from stream wrapper is skipped because there may not be any bytes to read.
        let restored_payload_ref = self.restored_payload.clone();
        let buffer_size = self.buffer_size.clone();
        let reader = self.reader.clone();

        Box::new(Box::pin(async move {
            let mut restored_payload = restored_payload_ref.lock().await;

            if let Some(payload) = restored_payload.as_ref() {
                let buffer = payload.to_owned();
                *restored_payload = None;
                return Ok(buffer);
            }

            let mut buffer = vec![0u8; buffer_size];
            let mut reader = reader.lock().await;

            return match reader.read(&mut buffer).await {
                Ok(read_size) => {
                    if read_size == 0 {
                        return Err(std::io::Error::new(
                            ErrorKind::BrokenPipe,
                            "Read size is 0. Probably connection broken.",
                        ));
                    }

                    let chunk = &buffer[0..read_size];
                    Ok(chunk.to_vec())
                }
                Err(error) => Err(std::io::Error::other(error)),
            };
        }))
    }

    fn write_chunk(&self, data: &[u8]) -> StreamResult<std::io::Result<()>> {
        let writer_ref = self.writer.clone();
        let data = data.to_vec();

        Box::new(Box::pin(async move {
            let mut writer = writer_ref.lock().await;
            writer.write_all(&data).await?;
            Ok(())
        }))
    }

    fn shutdown(&self) -> StreamResult<std::io::Result<()>> {
        let stream_ref = self.stream.clone();

        Box::new(Box::pin(async move {
            let mut stream = stream_ref.lock().await;
            stream.shutdown().await?;
            Ok(())
        }))
    }
}
