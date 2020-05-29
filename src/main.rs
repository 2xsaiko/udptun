use chrono::Duration;
use clap::{app_from_crate, Arg};

use crate::client::ClientParams;
use crate::common::{Format, IpMode};
use crate::server::ServerParams;

mod server;
mod client;
mod common;
mod cache;
mod server_cache;
mod sourcefmt;
mod output;

mod proto {
  pub const PROTO_VERSION: u8 = 0x01;

  pub const PACKET_CONNECT: u8 = 0x00;
  pub const PACKET_CONN_ACK: u8 = 0x01;
  pub const PACKET_DATA: u8 = 0x10;

  pub const TYPE_SERVER: u8 = 0x00;
  pub const TYPE_CLIENT: u8 = 0x01;
}

#[tokio::main]
async fn main() {
  let matches = app_from_crate!()
    .arg(Arg::with_name("target").short('T').long("target").value_name("ADDRESS").about("Specifies that this is the end of the tunnel the actual server is at; the specified address is the one of the actual server to proxy").conflicts_with("entry"))
    .arg(Arg::with_name("entry").short('E').long("entry").value_name("ADDRESS").about("Specifies that this is the tunnel entry point; the specified address is the one clients connect to"))
    .arg(Arg::with_name("timeout").short('x').long("timeout").default_value("3600").value_name("SECS").about("Time in seconds after the last received packet after which a connection is determined closed"))
    .arg(Arg::with_name("bufsize").short('b').long("bufsize").default_value("65536").value_name("SIZE").about("Packet buffer size, if smaller than packets sent they will get truncated"))
    .arg(Arg::with_name("listen").short('l').long("listen").value_name("ADDRESS").about("The address/port to use for communication inside the tunnel").required_unless("remote"))
    .arg(Arg::with_name("remote").short('r').long("remote").value_name("ADDRESS").about("Specifies the address of the other end of the tunnel").required_unless("listen"))
    .arg(Arg::with_name("source-format").long("source-format").value_name("ADDRESS-FMT").about("Specifies the IP address range for created dummy client sockets").requires("target"))
    .arg(Arg::with_name("ipv4").short('4').conflicts_with("ipv6").about("Exclusively use IPv4"))
    .arg(Arg::with_name("ipv6").short('6').about("Exclusively use IPv6"))
    .arg(Arg::with_name("log-data").short('L').long("log-data").about("Print a log line per data packet transferred"))
    .arg(Arg::with_name("format").short('f').long("format").value_name("FORMAT").requires("log-data").about("Set the log line format"))
    .arg(Arg::with_name("print-data-buffer").short('B').long("print-data-buffer").about("Print the contents of the data buffer for each packet transferred"))
    .arg(Arg::with_name("verbose").short('v').long("verbose").about("Print more information").multiple_occurrences(true))
    .get_matches();

  let target = matches.value_of("target");
  let entry = matches.value_of("entry");
  let remote = matches.value_of("remote");
  let timeout = Duration::minutes(matches.value_of("timeout").unwrap().parse().unwrap());
  let bufsize = matches.value_of("bufsize").unwrap().parse().unwrap();
  let listen = matches.value_of("listen");
  let source_format = matches.value_of("source-format").map(|s| s.parse().unwrap());
  let verbosity = matches.occurrences_of("verbose");
  let ip_mode = if matches.is_present("ipv4") { IpMode::V4Only } else if matches.is_present("ipv6") { IpMode::V6Only } else { IpMode::Both };
  let log_data = matches.is_present("log-data");
  let format = if log_data {
    if let Some(s) = matches.value_of("format") {
      Some(Format::Custom(s))
    } else {
      Some(Format::Default)
    }
  } else { None };
  let print_data_buffer = matches.is_present("print-data-buffer");

  if let Some(target) = target {
    let params = ServerParams { target, remote, bufsize, timeout, tunnel_addr: listen, source_format, mode: ip_mode, format, print_data_buffer };
    server::start_server(params).await;
  } else if let Some(entry) = entry {
    let params = ClientParams { entry, remote, timeout, bufsize, tunnel_addr: listen, mode: ip_mode, format, print_data_buffer };
    client::start_client(params).await;
  } else {
    eprintln!("One of -T/--target, -E/--entry is required!");
    std::process::exit(1);
  }
}

