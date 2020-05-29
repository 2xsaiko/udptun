use std::{fmt, io};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;

use chrono::Duration;
use rand::prelude::{SliceRandom, ThreadRng};
use tokio::net::{ToSocketAddrs, UdpSocket};

use crate::{common, output};
use crate::cache::{Cache, SocketId};
use crate::common::{Format, IpMode, respond_connect, setup_tunnel_socket};
use crate::output::Alignment;
use crate::proto::*;

pub struct ClientParams<'a, T, U, V>
    where T: ToSocketAddrs,
          U: ToSocketAddrs,
          V: ToSocketAddrs {
    pub entry: T,
    pub remote: Option<U>,
    pub timeout: Duration,
    pub bufsize: usize,
    pub tunnel_addr: Option<V>,
    pub mode: IpMode,
    pub format: Option<Format<'a>>,
    pub print_data_buffer: bool,
}

pub async fn start_client<T, U, V>(params: ClientParams<'_, T, U, V>)
    where T: ToSocketAddrs,
          U: ToSocketAddrs,
          V: ToSocketAddrs {
    let mut buffer = vec![0; params.bufsize];
    let mut external_socket = UdpSocket::bind(params.entry).await.unwrap();
    let mut tunnel_socket = setup_tunnel_socket(params.tunnel_addr, params.remote, params.mode, &mut buffer, TYPE_SERVER).await;
    let mut cache = Cache::new(params.timeout);
    let data_table = params.format.map(|f| output::Table::<OutputColumn>::parse_spec(f.with_default("[tunnel %D] client: %C cid: %i dbuf: %l")).unwrap());

    loop {
        match poll_sockets(&tunnel_socket, &external_socket, &mut buffer[2..]).await {
            (dir, Ok((size, sender_addr))) => {
                match dir {
                    Direction::FromTunnel => {
                        let buffer = &mut buffer[2..];
                        if size == 0 { continue; }
                        match buffer[0] {
                            PACKET_CONNECT => {
                                respond_connect(&mut tunnel_socket, sender_addr, buffer, TYPE_CLIENT).await;
                            }
                            PACKET_DATA => {
                                let id = buffer[1];
                                let buffer = &mut buffer[2..size];
                                if let Some(SocketId { addr, .. }) = cache.get_by_id(id) {
                                    if let Some(data_table) = &data_table {
                                        let data = DataPacketInfo {
                                            to_tunnel: false,
                                            client: addr,
                                            cid: id,
                                            tunnel: tunnel_socket.local_addr().ok(),
                                            data_len: buffer.len(),
                                        };
                                        println!("{}", data_table.bind(&data));
                                    }
                                    if let Err(e) = external_socket.send_to(&buffer, addr).await {
                                        eprintln!("failed to send packet: {}", e);
                                    }
                                } else {
                                    eprintln!("received packet for id {}, but it doesn't exist!", id);
                                }
                            }
                            _ => eprintln!("ignoring invalid packet type ${:02X}", buffer[0])
                        }
                    }
                    Direction::IntoTunnel => {
                        let id = cache.get_or_insert_by_addr(sender_addr).unwrap().id;
                        buffer[0] = PACKET_DATA;
                        buffer[1] = id;
                        if let Some(data_table) = &data_table {
                            let data = DataPacketInfo {
                                to_tunnel: true,
                                client: sender_addr,
                                cid: id,
                                tunnel: tunnel_socket.local_addr().ok(),
                                data_len: size,
                            };
                            println!("{}", data_table.bind(&data));
                        }
                        if let Err(e) = tunnel_socket.send(&buffer[..size + 2]).await {
                            eprintln!("failed to send packet: {}", e);
                        }
                    }
                }
            }
            (dir, Err(e)) => {
                eprintln!("recv error from {}, ignoring: {}", dir, e);
            }
        }
    }
}

async fn poll_sockets(tunnel_socket: &UdpSocket, external_socket: &UdpSocket, buf: &mut [u8]) -> (Direction, io::Result<(usize, SocketAddr)>) {
    let mut all = [
        (Direction::FromTunnel, tunnel_socket),
        (Direction::IntoTunnel, external_socket),
    ];
    all.shuffle(&mut ThreadRng::default());

    let (d, r) = common::poll_sockets(&all, buf).await;
    (*d, r)
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

struct DataPacketInfo {
    to_tunnel: bool,
    client: SocketAddr,
    cid: u8,
    tunnel: Option<SocketAddr>,
    data_len: usize,
}

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
enum OutputColumn {
    Direction,
    RevDirection,
    Client,
    ClientId,
    ClientAddr,
    TunnelAddr,
    DataLen,
}

impl output::Column for OutputColumn {
    type Data = DataPacketInfo;

    fn by_char(ch: char) -> Option<Self> {
        match ch {
            'd' => Some(OutputColumn::Direction),
            'D' => Some(OutputColumn::RevDirection),
            'c' => Some(OutputColumn::Client),
            'i' => Some(OutputColumn::ClientId),
            'C' => Some(OutputColumn::ClientAddr),
            't' => Some(OutputColumn::TunnelAddr),
            'l' => Some(OutputColumn::DataLen),
            _ => None,
        }
    }

    fn to_string<'a>(&'a self, data: &'a Self::Data) -> Cow<'a, str> {
        match self {
            OutputColumn::Direction => if data.to_tunnel { "=>" } else { "<=" }.into(),
            OutputColumn::RevDirection => if data.to_tunnel { "<=" } else { "=>" }.into(),
            OutputColumn::Client => format!("{}@{}", data.cid, OutputColumn::TunnelAddr.to_string(data)).into(),
            OutputColumn::ClientId => format!("{}", data.cid).into(),
            OutputColumn::ClientAddr => format!("{}", data.client).into(),
            OutputColumn::TunnelAddr => if let Some(tunnel) = data.tunnel { format!("{}", tunnel).into() } else { "???".into() },
            OutputColumn::DataLen => format!("{}", data.data_len).into(),
        }
    }

    fn constant_size(&self) -> bool {
        *self == OutputColumn::Direction
    }

    fn alignment(&self) -> Alignment {
        match self {
            OutputColumn::ClientId | OutputColumn::DataLen => Alignment::Right,
            _ => Alignment::Left
        }
    }
}