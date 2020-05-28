use std::{fmt, io};
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::task::Poll;

use chrono::Duration;
use rand::prelude::{SliceRandom, ThreadRng};
use tokio::future::poll_fn;
use tokio::net::UdpSocket;

use crate::{PACKET_CONN_ACK, PACKET_CONNECT, PACKET_DATA, TYPE_CLIENT, TYPE_SERVER};
use crate::cache::{Cache, SocketId};

pub async fn start_client(entry: SocketAddr, remote: Option<SocketAddr>, timeout: Duration, bufsize: usize, tunnel_addr: Option<SocketAddr>,verbosity: u64) {
    let mut buffer = vec![0; bufsize];

    let mut external_socket = UdpSocket::bind(entry).await.unwrap();
    let mut tunnel_socket = UdpSocket::bind(crate::select_addr_or_default_by(tunnel_addr, || remote.unwrap())).await.unwrap();
    if let Some(remote) = remote {
        tunnel_socket.connect(remote).await.unwrap();
    }
    if tunnel_addr.is_none() {
        crate::connect(&mut tunnel_socket, &mut buffer, TYPE_SERVER).await;
    }

    let mut cache = Cache::new(timeout);

    loop {
        match poll_sockets(&tunnel_socket, &external_socket, &mut buffer[2..]).await {
            (dir, Ok((size, source_addr))) => {
                match dir {
                    Direction::FromTunnel => {
                        let buffer = &mut buffer[2..];
                        if size == 0 { continue; }
                        match buffer[0] {
                            PACKET_CONNECT => {
                                buffer[0] = PACKET_CONN_ACK;
                                buffer[1] = TYPE_CLIENT;
                                buffer[2] = 0x01; // version
                                tunnel_socket.connect(source_addr).await.unwrap();
                                tunnel_socket.send(&buffer[..3]).await.unwrap();
                            }
                            PACKET_DATA => {
                                let id = buffer[1];
                                let buffer = &mut buffer[2..size];
                                if let Some(SocketId { addr, .. }) = cache.get_by_id(id) {
                                    println!("sending {} bytes to {}", size + 1, addr);
                                    external_socket.send_to(&buffer, addr).await.unwrap();
                                } else {
                                    eprintln!("received packet for id {}, but it doesn't exist!", id);
                                }
                            }
                            _ => eprintln!("ignoring invalid packet type ${:02X}", buffer[0])
                        }
                    }
                    Direction::IntoTunnel => {
                        buffer[0] = PACKET_DATA;
                        buffer[1] = cache.get_or_insert_by_addr(source_addr).unwrap().id;
                        tunnel_socket.send(&buffer[..size + 2]).await.unwrap();
                    }
                }
            }
            (dir, Err(e)) => {
                eprintln!("recv error from {}, ignoring: {}", dir, e);
            }
        }
        // match tunnel_socket.recv_from(&mut buffer[1..]).await {
        //     Ok((size, src)) => {
        //         let buffer = &mut buffer[..size + 1];
        //         if src != remote {
        //             let id = cache.get_or_insert_by_addr(src).unwrap().id;
        //             buffer[0] = id;
        //             println!("received packet from {} ({}), {} bytes", src, id, size);
        //             println!("sending {} bytes to {}", buffer.len(), remote);
        //             match tunnel_socket.send_to(buffer, remote).await {
        //                 Ok(written) if written != size + 1 => {
        //                     println!("warning: wrote {} bytes, but received {} (+2)!", written, size)
        //                 }
        //                 Err(e) => {
        //                     eprintln!("write error: {}", e);
        //                 }
        //                 _ => {}
        //             }
        //         } else {
        //             let id = buffer[0];
        //             let buffer = &buffer[1..];
        //             if let Some(SocketId { addr, .. }) = cache.get_by_id(id) {
        //                 println!("received packet from tunnel");
        //                 println!("sending {} bytes to {}", size + 1, addr);
        //                 tunnel_socket.send_to(buffer, addr).await.unwrap();
        //             } else {
        //                 eprintln!("received packet for id {}, but it doesn't exist!", id);
        //             }
        //         }
        //     }
        //     Err(e) => {
        //         eprintln!("recv error, ignoring: {}", e);
        //     }
        // }
    }
}

async fn poll_sockets(tunnel_socket: &UdpSocket, external_socket: &UdpSocket, buf: &mut [u8]) -> (Direction, io::Result<(usize, SocketAddr)>) {
    let mut all = [
        (Direction::FromTunnel, tunnel_socket),
        (Direction::IntoTunnel, external_socket),
    ];
    all.shuffle(&mut ThreadRng::default());

    poll_fn(|cx| {
        all.iter().filter_map(|&(dir, sock)| match sock.poll_recv_from(cx, buf) {
            Poll::Ready(r) => Some((dir, r)),
            Poll::Pending => None,
        }).next().map(Poll::Ready).unwrap_or(Poll::Pending)
    }).await
}

#[derive(Copy, Clone)]
enum Direction {
    FromTunnel,
    IntoTunnel,
}

impl Display for Direction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Direction::FromTunnel => write!(f, "tunnel"),
            Direction::IntoTunnel => write!(f, "client"),
        }
    }
}