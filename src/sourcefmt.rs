use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4};
use std::ops::Add;
use std::str::FromStr;

use itertools::Itertools;
use rand::{Rng, RngCore};
use rand::distributions::uniform::SampleUniform;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SourceFormat {
    V4(SourceFormatV4),
    V6(SourceFormatV6),
}

impl SourceFormat {
    pub fn get_addr(&self, rand: impl RngCore) -> SocketAddr {
        match self {
            SourceFormat::V4(f) => SocketAddr::V4(f.get_addr(rand)),
            SourceFormat::V6(_) => unimplemented!(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SourceFormatV4 {
    ip: (Range<u8>, Range<u8>, Range<u8>, Range<u8>),
    port: Range<u16>,
}

impl SourceFormatV4 {
    pub fn get_addr(&self, mut rand: impl RngCore) -> SocketAddrV4 {
        let u1 = self.ip.0.get_random(&mut rand);
        let u2 = self.ip.1.get_random(&mut rand);
        let u3 = self.ip.2.get_random(&mut rand);
        let u4 = self.ip.3.get_random(&mut rand);
        let port = self.port.get_random(&mut rand);
        SocketAddrV4::new(Ipv4Addr::new(u1, u2, u3, u4), port)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SourceFormatV6 {
    // yeah not doing ipv6 range parsing lol
    ip: Ipv6Addr,
    port: Range<u16>,
}

impl FromStr for SourceFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse() {
            Ok(v) => Ok(SourceFormat::V4(v)),
            Err(_) => match s.parse() {
                Ok(v) => Ok(SourceFormat::V6(v)),
                Err(e) => Err(e)
            }
        }
    }
}

impl FromStr for SourceFormatV4 {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.splitn(2, ':').collect();
        if let [addr, port] = *parts {
            let addr_parts: Vec<_> = addr.splitn(4, '.').map(parse_range).try_collect().map_err(|_| ())?;
            if let [u1, u2, u3, u4] = *addr_parts {
                let port = parse_range(port).map_err(|_| ())?;
                Ok(SourceFormatV4 { ip: (u1, u2, u3, u4), port })
            } else { Err(()) }
        } else { Err(()) }
    }
}

impl FromStr for SourceFormatV6 {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

fn parse_range<T: FromStr + Copy>(s: &str) -> Result<Range<T>, <T as FromStr>::Err> {
    let parts = s.splitn(2, '-')
        .map(|s| s.parse())
        .fold_results(Vec::new(), |mut acc, a| {
            acc.push(a);
            acc
        })?;
    if let [a, b] = *parts {
        Ok(Range::Exclusive { start: a, end: b })
    } else if let [a] = *parts {
        Ok(Range::Single(a))
    } else { panic!("haha too lazy to handle properly") }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Range<T> {
    Single(T),
    Exclusive {
        start: T,
        end: T,
    },
}

impl<T> Range<T>
    where T: Copy + SampleUniform {
    pub fn get_random(&self, mut rand: impl RngCore) -> T {
        match *self {
            Range::Single(s) => s,
            Range::Exclusive { start, end } => rand.gen_range(start, end),
        }
    }
}