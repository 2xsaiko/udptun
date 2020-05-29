use std::{fmt, io};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;

use chrono::Duration;
use rand::prelude::{SliceRandom, ThreadRng};
use tokio::net::{ToSocketAddrs, UdpSocket};

use crate::{common, output};
use crate::common::{default_listen_ip, Format, IpMode, respond_connect, setup_tunnel_socket};
use crate::output::Alignment;
use crate::proto::*;
use crate::server_cache::{Cache, CacheEntry};
use crate::sourcefmt::SourceFormat;

pub struct ServerParams<'a, T, U, V>
    where T: ToSocketAddrs,
          U: ToSocketAddrs,
          V: ToSocketAddrs {
    pub target: T,
    pub remote: Option<U>,
    pub bufsize: usize,
    pub timeout: Duration,
    pub tunnel_addr: Option<V>,
    pub source_format: Option<SourceFormat>,
    pub mode: IpMode,
    pub format: Option<Format<'a>>,
    pub print_data_buffer: bool,
}

pub async fn start_server<T, U, V>(params: ServerParams<'_, T, U, V>)
    where T: ToSocketAddrs,
          U: ToSocketAddrs,
          V: ToSocketAddrs {
    let mut buffer = vec![0; params.bufsize];
    let mut tunnel_socket = setup_tunnel_socket(params.tunnel_addr, params.remote, params.mode, &mut buffer, TYPE_CLIENT).await.expect("failed to setup tunnel");
    let mut cache: Cache = Cache::new(params.timeout);
    let data_output = params.format.map(|f| output::TableFormat::<OutputColumn>::parse_spec(f.with_default("[%d tunnel] client: %c lsock: %a dbuf: %l")).expect("failed to parse data log format"));

    loop {
        match poll_sockets(&tunnel_socket, &cache, &mut buffer[2..]).await {
            (dir, Ok((size, sender_addr))) => {
                match dir {
                    Direction::FromTunnel => {
                        let buffer = &mut buffer[2..];
                        if size == 0 { continue; }
                        match buffer[0] {
                            PACKET_CONNECT => {
                                respond_connect(&mut tunnel_socket, sender_addr, buffer, TYPE_SERVER).await;
                            }
                            PACKET_DATA => {
                                let buffer = &mut buffer[..size];
                                if buffer.len() < 2 {
                                    eprintln!("packet from {} too small for data, ignoring", sender_addr);
                                    continue;
                                }
                                let id = ConnId { from: sender_addr, cid: buffer[1] };
                                let socket = if let Some(CacheEntry { socket, .. }) = cache.get_by_id_mut(id) {
                                    socket
                                } else {
                                    match create_socket(&params.target, &params.source_format, params.mode).await {
                                        Ok(s) => &mut cache.insert(id, s).socket,
                                        Err(e) => {
                                            eprintln!("failed to open client socket: {}", e);
                                            continue;
                                        }
                                    }
                                };
                                if let Some(data_table) = &data_output {
                                    let info = DataPacketInfo {
                                        to_tunnel: false,
                                        client: id,
                                        tunnel_socket: socket.local_addr().ok(),
                                        data_len: buffer.len() - 2,
                                    };
                                    println!("{}", data_table.bind(&info));
                                }
                                if let Err(e) = socket.send(&buffer[2..]).await {
                                    eprintln!("failed to send packet: {}", e);
                                }
                            }
                            _ => eprintln!("ignoring invalid packet type ${:02X} from {}", buffer[0], sender_addr)
                        }
                    }
                    Direction::IntoTunnel(id) => {
                        buffer[0] = PACKET_DATA;
                        buffer[1] = id.cid;
                        if let Some(data_table) = &data_output {
                            let info = DataPacketInfo {
                                to_tunnel: true,
                                client: id,
                                tunnel_socket: cache.get_by_id_mut(id).and_then(|s| s.socket.local_addr().ok()),
                                data_len: size,
                            };
                            println!("{}", data_table.bind(&info));
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

async fn poll_sockets(tunnel_socket: &UdpSocket, cache: &Cache, buf: &mut [u8]) -> (Direction, io::Result<(usize, SocketAddr)>) {
    let mut all = Vec::with_capacity(cache.len_max() + 1);
    all.push((Direction::FromTunnel, tunnel_socket));
    all.extend(cache.iter().map(|e| (Direction::IntoTunnel(e.id), &e.socket)));
    all.shuffle(&mut ThreadRng::default());

    let (d, r) = common::poll_sockets(&all, buf).await;
    (*d, r)
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
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.cid, self.from)
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Direction::FromTunnel => write!(f, "tunnel"),
            Direction::IntoTunnel(id) => write!(f, "target (connection from {})", id),
        }
    }
}

async fn create_socket(target: impl ToSocketAddrs, sf: &Option<SourceFormat>, mode: IpMode) -> io::Result<UdpSocket> {
    let a = sf.map(|sf| sf.get_addr(ThreadRng::default())).unwrap_or_else(|| default_listen_ip(mode));
    println!("creating socket on {}", a);
    let socket = UdpSocket::bind(a).await?;
    socket.connect(target).await?;
    Ok(socket)
}

struct DataPacketInfo {
    to_tunnel: bool,
    client: ConnId,
    tunnel_socket: Option<SocketAddr>,
    data_len: usize,
}

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
enum OutputColumn {
    Direction,
    RevDirection,
    Client,
    ClientId,
    Peer,
    TunnelSocket,
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
            'p' => Some(OutputColumn::Peer),
            'a' => Some(OutputColumn::TunnelSocket),
            'l' => Some(OutputColumn::DataLen),
            _ => None,
        }
    }

    fn to_string<'a>(&'a self, data: &'a Self::Data) -> Cow<'a, str> {
        match self {
            OutputColumn::Direction => if data.to_tunnel { "=>" } else { "<=" }.into(),
            OutputColumn::RevDirection => if data.to_tunnel { "<=" } else { "=>" }.into(),
            OutputColumn::Client => format!("{}", data.client).into(),
            OutputColumn::ClientId => format!("{}", data.client.cid).into(),
            OutputColumn::Peer => format!("{}", data.client.from).into(),
            OutputColumn::TunnelSocket => if let Some(s) = data.tunnel_socket { format!("{}", s).into() } else { "???".into() },
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