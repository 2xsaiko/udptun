use std::{fmt, io};
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::task::Poll;

use chrono::Duration;
use rand::prelude::{SliceRandom, ThreadRng};
use rand::Rng;
use tokio::future::poll_fn;
use tokio::net::UdpSocket;

use crate::{PACKET_CONN_ACK, PACKET_CONNECT, PACKET_DATA, TYPE_CLIENT, TYPE_SERVER};
use crate::server_cache::{Cache, CacheEntry};
use crate::sourcefmt::SourceFormat;

pub async fn start_server(target: SocketAddr, remote: Option<SocketAddr>, bufsize: usize, timeout: Duration, tunnel_addr: Option<SocketAddr>, source_format: Option<SourceFormat>, verbosity: u64) {
    let mut buffer = vec![0; bufsize];

    let mut tunnel_socket = UdpSocket::bind(crate::select_addr_or_default_by(tunnel_addr, || remote.unwrap())).await.unwrap();
    if let Some(remote) = remote {
        tunnel_socket.connect(remote).await.unwrap();
    }
    if tunnel_addr.is_none() {
        crate::connect(&mut tunnel_socket, &mut buffer, TYPE_CLIENT).await;
    }

    let mut cache: Cache = Cache::new(timeout);

    loop {
        match poll_sockets(&tunnel_socket, &cache, &mut buffer[2..]).await {
            (dir, Ok((size, source_addr))) => {
                match dir {
                    Direction::FromTunnel => {
                        let buffer = &mut buffer[2..];
                        if size == 0 { continue; }
                        match buffer[0] {
                            PACKET_CONNECT => {
                                buffer[0] = PACKET_CONN_ACK;
                                buffer[1] = TYPE_SERVER;
                                buffer[2] = 0x01; // version
                                tunnel_socket.connect(source_addr).await.unwrap();
                                tunnel_socket.send(&buffer[..3]).await.unwrap();
                            }
                            PACKET_DATA => {
                                let buffer = &mut buffer[..size];
                                if buffer.len() < 2 {
                                    eprintln!("packet from {} too small for data, ignoring", source_addr);
                                    continue;
                                }
                                let id = ConnId { from: source_addr, cid: buffer[1] };
                                println!("relaying packet from {}", id);
                                let socket = if let Some(CacheEntry { socket, .. }) = cache.get_by_id_mut(id) {
                                    socket
                                } else {
                                    &mut cache.insert(id, create_socket(target, &source_format).await).socket
                                };
                                println!("sending {} bytes to {}", buffer.len() - 2, target);
                                socket.send_to(&buffer[2..], target).await.unwrap();
                            }
                            _ => eprintln!("ignoring invalid packet type ${:02X}", buffer[0])
                        }
                    }
                    Direction::IntoTunnel(id) => {
                        buffer[0] = PACKET_DATA;
                        buffer[1] = id.cid;
                        println!("relaying packet from target server to {}, through {}", id, id.from);
                        tunnel_socket.send_to(&buffer[..size + 2], id.from).await.unwrap();
                    }
                }
            }
            (dir, Err(e)) => {
                eprintln!("recv error from {}, ignoring: {}", dir, e);
            }
        }
    }
}

async fn poll_sockets(tunnel_socket: &UdpSocket, cache: &Cache, buf: &mut [u8]) -> (Direction, io::Result<(usize, SocketAddr)>) {
    let mut all = Vec::with_capacity(cache.len_max() + 1);
    all.push((Direction::FromTunnel, tunnel_socket));
    all.extend(cache.iter().map(|e| (Direction::IntoTunnel(e.id), &e.socket)));
    all.shuffle(&mut ThreadRng::default());

    poll_fn(|cx| {
        all.iter().filter_map(|&(dir, sock)| match sock.poll_recv_from(cx, buf) {
            Poll::Ready(r) => Some((dir, r)),
            Poll::Pending => None,
        }).next().map(Poll::Ready).unwrap_or(Poll::Pending)
    }).await
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ConnId {
    from: SocketAddr,
    cid: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Direction {
    FromTunnel,
    IntoTunnel(ConnId),
}

impl Display for ConnId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.cid, self.from)
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Direction::FromTunnel => write!(f, "tunnel"),
            Direction::IntoTunnel(id) => write!(f, "target (connection from {})", id),
        }
    }
}

async fn create_socket(target: SocketAddr, sf: &Option<SourceFormat>) -> UdpSocket {
    let a = if let Some(sf) = sf {
        sf.get_addr(ThreadRng::default())
    } else {
        crate::get_ip_version_default_addr(target)
    };
    println!("Creating socket on {}", a);
    let socket = fuck_you::create(a).unwrap();
    // socket.connect(target).await.unwrap();
    socket
}

mod fuck_you {
    use std::io;
    use std::net::SocketAddr;

    use net2::{UdpBuilder, unix::UnixUdpBuilderExt};

    pub(crate) fn create(addr: SocketAddr) -> io::Result<tokio::net::UdpSocket> {
        let socket = (if addr.is_ipv4() { UdpBuilder::new_v4()? } else { UdpBuilder::new_v6()? })
            .reuse_address(true)?
            .reuse_port(true)?
            .bind(addr)?;
        tokio::net::UdpSocket::from_std(socket)
    }
}