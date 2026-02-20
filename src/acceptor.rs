use std::fmt;
use std::marker::Unpin;

use crate::handshake::handshake;
use crate::runtime::{AsyncRead, AsyncReadExt, AsyncWrite};
use crate::TlsStream;

/// A wrapper around a `native_tls::TlsAcceptor`, providing an async `accept`
/// method.
///
/// # Example
///
/// ```no_run
/// # #[cfg(feature = "runtime-smol")]
/// # fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> { smol::block_on(async {
/// #
/// use smol::prelude::*;
/// use smol::net::TcpListener;
/// use smol::fs::File;
/// use async_native_tls::TlsAcceptor;
///
/// let key = File::open("tests/identity.pfx").await?;
/// let acceptor = TlsAcceptor::new(key, "hello").await?;
/// let listener = TcpListener::bind("127.0.0.1:8443").await?;
/// let mut incoming = listener.incoming();
///
/// while let Some(stream) = incoming.next().await {
///     let acceptor = acceptor.clone();
///     let stream = stream?;
///     smol::spawn(async move {
///         let stream = acceptor.accept(stream).await.unwrap();
///         // handle stream here
///     });
/// }
/// #
/// # Ok(()) }) }
/// # #[cfg(feature = "runtime-tokio")]
/// # fn main() {}
/// ```
#[derive(Clone)]
pub struct TlsAcceptor(native_tls::TlsAcceptor);

/// An error returned from creating an acceptor.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// NativeTls error.
    #[error("NativeTls({})", 0)]
    NativeTls(#[from] native_tls::Error),
    /// Io error.
    #[error("Io({})", 0)]
    Io(#[from] std::io::Error),
}

impl TlsAcceptor {
    /// Create a new TlsAcceptor based on an identity file and matching password.
    pub async fn new<R, S>(mut file: R, password: S) -> Result<Self, Error>
    where
        R: AsyncRead + Unpin,
        S: AsRef<str>,
    {
        let mut identity = vec![];
        file.read_to_end(&mut identity).await?;

        let identity = native_tls::Identity::from_pkcs12(&identity, password.as_ref())?;
        Ok(TlsAcceptor(native_tls::TlsAcceptor::new(identity)?))
    }

    /// Accepts a new client connection with the provided stream.
    ///
    /// This function will internally call `TlsAcceptor::accept` to connect
    /// the stream and returns a future representing the resolution of the
    /// connection operation. The returned future will resolve to either
    /// `TlsStream<S>` or `Error` depending if it's successful or not.
    ///
    /// This is typically used after a new socket has been accepted from a
    /// `TcpListener`. That socket is then passed to this function to perform
    /// the server half of accepting a client connection.
    pub async fn accept<S>(&self, stream: S) -> Result<TlsStream<S>, native_tls::Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let stream = handshake(move |s| self.0.accept(s), stream).await?;
        Ok(stream)
    }
}

impl fmt::Debug for TlsAcceptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsAcceptor").finish()
    }
}

impl From<native_tls::TlsAcceptor> for TlsAcceptor {
    fn from(inner: native_tls::TlsAcceptor) -> TlsAcceptor {
        TlsAcceptor(inner)
    }
}

#[cfg(all(test, feature = "runtime-smol"))]
mod tests {
    use super::*;
    use crate::runtime::AsyncWriteExt;
    use crate::TlsConnector;
    use smol::fs::File;
    use smol::net::{TcpListener, TcpStream};
    use smol::stream::StreamExt;

    #[test]
    fn test_acceptor() {
        smol::block_on(async {
            let key = File::open("tests/identity.pfx").await.unwrap();
            let acceptor = TlsAcceptor::new(key, "hello").await.unwrap();
            let listener = TcpListener::bind("127.0.0.1:8443").await.unwrap();
            smol::spawn(async move {
                let mut incoming = listener.incoming();

                while let Some(stream) = incoming.next().await {
                    let acceptor = acceptor.clone();
                    let stream = stream.unwrap();
                    smol::spawn(async move {
                        let mut stream = acceptor.accept(stream).await.unwrap();
                        stream.write_all(b"hello").await.unwrap();
                    })
                    .detach();
                }
            })
            .detach();

            let stream = TcpStream::connect("127.0.01:8443").await.unwrap();
            let connector = TlsConnector::new().danger_accept_invalid_certs(true);

            let mut stream = connector.connect("127.0.0.1", stream).await.unwrap();
            let mut res = Vec::new();
            stream.read_to_end(&mut res).await.unwrap();
            assert_eq!(res, b"hello");
        })
    }
}
