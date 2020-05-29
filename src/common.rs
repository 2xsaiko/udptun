use std::fmt::{Display, Formatter};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::ops::Deref;
use std::task::Poll;

use thiserror::Error;
use tokio::future::poll_fn;
use tokio::io;
use tokio::net::{ToSocketAddrs, UdpSocket};

use crate::proto::*;

pub async fn setup_tunnel_socket(tunnel_addr: Option<impl ToSocketAddrs>, remote: Option<impl ToSocketAddrs>, mode: IpMode, buffer: &mut [u8], remote_type: u8) -> Result<UdpSocket, Error> {
    let mut tunnel_socket = if let Some(tunnel_addr) = &tunnel_addr {
        UdpSocket::bind(tunnel_addr).await
    } else {
        UdpSocket::bind(default_listen_ip(mode)).await
    }.map_err(Error::TunnelSocketBind)?;
    if let Some(remote) = remote {
        tunnel_socket.connect(remote).await.map_err(Error::RemoteConnect)?;
    }
    if tunnel_addr.is_none() {
        send_connect(&mut tunnel_socket, buffer, remote_type).await?;
    }
    Ok(tunnel_socket)
}

pub async fn send_connect(tunnel_socket: &mut UdpSocket, buffer: &mut [u8], remote_type: u8) -> Result<(), Error> {
    buffer[0] = PACKET_CONNECT;
    tunnel_socket.send(&buffer[..1]).await.map_err(Error::ConnectSend)?;
    let len = tunnel_socket.recv(buffer).await.map_err(Error::ConnectRecv)?;
    let expected = [PACKET_CONN_ACK, remote_type, 0x01];
    if buffer[..len] != expected {
        return Err(Error::ConnectResponse {
            response: HexFormat(buffer[..len].into()),
            expected: HexFormat(expected),
        });
    }
    Ok(())
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to bind tunnel socket")]
    TunnelSocketBind(#[source] io::Error),
    #[error("failed to connect to remote")]
    RemoteConnect(#[source] io::Error),
    #[error("failed to send connect packet")]
    ConnectSend(#[source] io::Error),
    #[error("failed to receive connect response")]
    ConnectRecv(#[source] io::Error),
    #[error("remote sent invalid response to connect: {response}, expected {expected}")]
    ConnectResponse { response: HexFormat<Vec<u8>>, expected: HexFormat<[u8; 3]> },
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct HexFormat<T>(T);

impl<T> HexFormat<T> {
    pub fn into_inner(self) -> T { self.0 }
}

impl<T> Deref for HexFormat<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target { &self.0 }
}

impl<T> Display for HexFormat<T>
    where for<'a> &'a T: IntoIterator<Item=&'a u8> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut iter = self.into_iter();
        if let Some(e) = iter.next() {
            write!(f, "{:02X}", e)?;
        }
        for e in iter {
            write!(f, " {:02X}", e)?;
        }
        Ok(())
    }
}

pub async fn respond_connect(tunnel_socket: &mut UdpSocket, sender_addr: SocketAddr, buffer: &mut [u8], typ: u8) {
    buffer[0] = PACKET_CONN_ACK;
    buffer[1] = typ;
    buffer[2] = PROTO_VERSION;
    println!("[connect]\tremote: {}", sender_addr);
    tunnel_socket.connect(sender_addr).await.expect("failed to connect to remote");
    tunnel_socket.send(&buffer[..3]).await.expect("failed to send connect response");
}

pub fn default_listen_ip(mode: IpMode) -> SocketAddr {
    match mode {
        IpMode::V4Only => SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into(),
        IpMode::Both | IpMode::V6Only => SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into(),
    }
}

pub async fn poll_sockets<'a, T>(sockets: &'a [(T, &UdpSocket)], buf: &mut [u8]) -> (&'a T, io::Result<(usize, SocketAddr)>) {
    poll_fn(|cx| {
        sockets.iter().filter_map(|(dir, sock)| match sock.poll_recv_from(cx, buf) {
            Poll::Ready(r) => Some((dir, r)),
            Poll::Pending => None,
        }).next().map(Poll::Ready).unwrap_or(Poll::Pending)
    }).await
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IpMode {
    Both,
    V4Only,
    V6Only,
}

pub enum Format<'a> {
    Default,
    Custom(&'a str),
}

impl<'a> Format<'a> {
    pub fn with_default(&'a self, default: &'a str) -> &'a str {
        match self {
            Format::Default => default,
            Format::Custom(c) => c,
        }
    }
}