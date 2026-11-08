//! The local connection: a Unix domain socket, or a named pipe on Windows.

use std::{io, path::Path};

use tokio::io::{AsyncRead, AsyncWrite};

/// Read half.
pub(crate) type Reader = Box<dyn AsyncRead + Send + Unpin>;
/// Write half.
pub(crate) type Writer = Box<dyn AsyncWrite + Send + Unpin>;

/// Connects to the socket at `path`.
#[cfg(unix)]
pub(crate) async fn connect(path: &Path) -> io::Result<(Reader, Writer)> {
    let (reader, writer) = tokio::net::UnixStream::connect(path).await?.into_split();
    Ok((Box::new(reader), Box::new(writer)))
}

/// Connects to the named pipe `path` (e.g. `\\.\pipe\omed`).
#[cfg(windows)]
pub(crate) async fn connect(path: &Path) -> io::Result<(Reader, Writer)> {
    let pipe = tokio::net::windows::named_pipe::ClientOptions::new().open(path)?;
    let (reader, writer) = tokio::io::split(pipe);
    Ok((Box::new(reader), Box::new(writer)))
}
