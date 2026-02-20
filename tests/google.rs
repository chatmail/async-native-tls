#![warn(rust_2018_idioms)]

use std::net::ToSocketAddrs;

use async_native_tls;
use env_logger;
use smol::net::TcpStream;
use smol::prelude::*;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

#[test]
fn fetch_google() {
    drop(env_logger::try_init());

    // First up, resolve google.com
    let addr = t!("google.com:443".to_socket_addrs()).next().unwrap();

    smol::block_on(async {
        let socket = TcpStream::connect(&addr).await.unwrap();

        // Send off the request by first negotiating an SSL handshake, then writing
        // of our request, then flushing, then finally read off the response.
        let connector = async_native_tls::TlsConnector::new();
        let url = url::Url::parse("https://google.com/").unwrap();
        let mut socket = t!(connector.connect(&url, socket).await);
        t!(socket.write_all(b"GET / HTTP/1.0\r\n\r\n").await);
        let mut data = Vec::new();
        t!(socket.read_to_end(&mut data).await);

        // any response code is fine
        assert!(data.starts_with(b"HTTP/1.0 "));

        let data = String::from_utf8_lossy(&data);
        let data = data.trim_end();
        assert!(data.ends_with("</html>") || data.ends_with("</HTML>"));
    })
}
