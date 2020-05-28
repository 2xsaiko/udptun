use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use chrono::Duration;
use clap::{app_from_crate, Arg};
use tokio::net::UdpSocket;
use itertools::Itertools;

mod server;
mod client;
mod cache;
mod server_cache;
mod sourcefmt;

const PACKET_CONNECT: u8 = 0x00;
const PACKET_CONN_ACK: u8 = 0x01;
const PACKET_DATA: u8 = 0x10;

const TYPE_SERVER: u8 = 0x00;
const TYPE_CLIENT: u8 = 0x01;

#[tokio::main]
async fn main() {
    let matches = app_from_crate!()
        .arg(Arg::with_name("target").short('T').long("target").value_name("ADDRESS").about("Specifies that this is the end of the tunnel the actual server is at; the specified address is the one of the actual server to proxy").conflicts_with("entry"))
        .arg(Arg::with_name("entry").short('E').long("entry").value_name("ADDRESS").about("Specifies that this is the tunnel entry point; the specified address is the one clients connect to"))
        .arg(Arg::with_name("timeout").short('x').long("timeout").default_value("3600").value_name("SECS").about("Time in seconds after the last received packet after which a connection is determined closed"))
        .arg(Arg::with_name("bufsize").short('b').long("bufsize").default_value("65536").value_name("SIZE").about("Packet buffer size, if smaller than packets sent they will get truncated"))
        .arg(Arg::with_name("listen").short('l').long("listen").value_name("ADDRESS").about("The address/port to use for communication inside the tunnel").required_unless("remote"))
        .arg(Arg::with_name("remote").short('r').long("remote").value_name("ADDRESS").about("Specifies the address of the other end of the tunnel").required_unless("listen"))
        .arg(Arg::with_name("source-format").long("source-format").value_name("ADDRESS-FMT"))
        .arg(Arg::with_name("verbose").short('v').long("verbose").about("Print more information").multiple_occurrences(true))
        .get_matches();

    let target = matches.value_of("target").map(|s| s.parse().unwrap());
    let entry = matches.value_of("entry").map(|s|s.parse().unwrap());
    let remote = matches.value_of("remote").map(|s| s.parse().unwrap());
    let timeout = Duration::minutes(matches.value_of("timeout").unwrap().parse().unwrap());
    let bufsize = matches.value_of("bufsize").unwrap().parse().unwrap();
    let listen = matches.value_of("listen").map(|s| s.parse().unwrap());
    let source_format = matches.value_of("source-format").map(|s|s.parse().unwrap());
    let verbosity = matches.occurrences_of("verbose");

    if let Some(target) = target {
        server::start_server(target, remote, bufsize, timeout, listen, source_format, verbosity).await;
    } else if let Some(entry) = entry {
        client::start_client(entry, remote, timeout, bufsize, listen, verbosity).await;
    } else {
        eprintln!("One of -T/--target, -E/--entry is required!");
        std::process::exit(1);
    }
}

fn select_addr_or_default_by(addr: Option<SocketAddr>, fallback_source: impl FnOnce() -> SocketAddr) -> SocketAddr {
    addr.unwrap_or_else(|| get_ip_version_default_addr(fallback_source()))
}

fn get_ip_version_default_addr(addr: SocketAddr) -> SocketAddr {
    let ip = if addr.is_ipv4() { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { IpAddr::V6(Ipv6Addr::UNSPECIFIED) };
    SocketAddr::new(ip, 0)
}

async fn connect(tunnel_socket: &mut UdpSocket, buffer: &mut [u8], remote_type: u8) {
    buffer[0] = PACKET_CONNECT;
    tunnel_socket.send(&buffer[..1]).await.unwrap();
    let len = tunnel_socket.recv(buffer).await.unwrap();
    let expected = [PACKET_CONN_ACK, remote_type, 0x01];
    if buffer[..len] != expected {
        eprintln!("remote sent invalid response to connect: {}, expected {}",
                  buffer[..len].iter().map(|b| format!("{:02X}", b)).join(" "),
                  expected.iter().map(|b| format!("{:02X}", b)).join(" "));
        std::process::exit(2);
    }
}

