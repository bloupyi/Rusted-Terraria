//! Multiplayer over TCP with our own messages. The host owns the world and the
//! monsters: a joining client gets the world as a `.wld`, then players, edits
//! and monsters are kept in step by small messages.
//!
//! Frames are `len: u32` then `type: u8` and a little-endian payload. Each
//! connection has a reader thread feeding one inbox and a writer thread fed by
//! a channel, so a large world upload never blocks the frame.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};

pub const DEFAULT_PORT: u16 = 7777;
/// Host's own player id; clients get 1, 2...
pub const HOST_ID: u8 = 0;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Welcome {
        id: u8,
        world: Vec<u8>,
    },
    Player {
        id: u8,
        x: f32,
        y: f32,
        vx: f32,
        vy: f32,
        facing_left: bool,
    },
    Tile {
        x: u32,
        y: u32,
        tile: u16,
        fx: i16,
        fy: i16,
    },
    Liquid {
        x: u32,
        y: u32,
        amount: u8,
        kind: u8,
    },
    /// Every monster: kind, top-left, facing left, hurt.
    Mobs(Vec<MobState>),
    /// A client hit the monster at this index of the last snapshot.
    Hit {
        index: u16,
        damage: u16,
    },
    Wall {
        x: u32,
        y: u32,
        wall: u16,
    },
    Leave {
        id: u8,
    },
    /// Host's time of day.
    Time(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobState {
    pub kind: u8,
    pub x: f32,
    pub y: f32,
    pub facing_left: bool,
    pub hurt: bool,
}

struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
}

struct In<'a>(&'a [u8]);

impl In<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (a, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }
}

impl Msg {
    /// The whole frame, length prefix included.
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(vec![0; 4]);
        match self {
            Msg::Welcome { id, world } => {
                o.u8(0).u8(*id).0.extend_from_slice(world);
            }
            Msg::Player { id, x, y, vx, vy, facing_left } => {
                o.u8(1).u8(*id).f32(*x).f32(*y).f32(*vx).f32(*vy).u8(*facing_left as u8);
            }
            Msg::Tile { x, y, tile, fx, fy } => {
                o.u8(2).u32(*x).u32(*y).u16(*tile).u16(*fx as u16).u16(*fy as u16);
            }
            Msg::Liquid { x, y, amount, kind } => {
                o.u8(3).u32(*x).u32(*y).u8(*amount).u8(*kind);
            }
            Msg::Mobs(mobs) => {
                o.u8(4).u16(mobs.len() as u16);
                for m in mobs {
                    o.u8(m.kind).f32(m.x).f32(m.y).u8(m.facing_left as u8 | (m.hurt as u8) << 1);
                }
            }
            Msg::Hit { index, damage } => {
                o.u8(5).u16(*index).u16(*damage);
            }
            Msg::Wall { x, y, wall } => {
                o.u8(8).u32(*x).u32(*y).u16(*wall);
            }
            Msg::Leave { id } => {
                o.u8(6).u8(*id);
            }
            Msg::Time(t) => {
                o.u8(7).f32(*t);
            }
        }
        let len = (o.0.len() - 4) as u32;
        o.0[..4].copy_from_slice(&len.to_le_bytes());
        o.0
    }

    /// One frame without its length prefix.
    pub fn decode(frame: &[u8]) -> Option<Msg> {
        let mut i = In(frame);
        Some(match i.u8()? {
            0 => {
                let id = i.u8()?;
                Msg::Welcome { id, world: i.0.to_vec() }
            }
            1 => Msg::Player {
                id: i.u8()?,
                x: i.f32()?,
                y: i.f32()?,
                vx: i.f32()?,
                vy: i.f32()?,
                facing_left: i.u8()? != 0,
            },
            2 => Msg::Tile { x: i.u32()?, y: i.u32()?, tile: i.u16()?, fx: i.u16()? as i16, fy: i.u16()? as i16 },
            3 => Msg::Liquid { x: i.u32()?, y: i.u32()?, amount: i.u8()?, kind: i.u8()? },
            4 => {
                let n = i.u16()?;
                let mut mobs = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    let (kind, x, y, bits) = (i.u8()?, i.f32()?, i.f32()?, i.u8()?);
                    mobs.push(MobState { kind, x, y, facing_left: bits & 1 != 0, hurt: bits & 2 != 0 });
                }
                Msg::Mobs(mobs)
            }
            5 => Msg::Hit { index: i.u16()?, damage: i.u16()? },
            8 => Msg::Wall { x: i.u32()?, y: i.u32()?, wall: i.u16()? },
            6 => Msg::Leave { id: i.u8()? },
            7 => Msg::Time(i.f32()?),
            _ => return None,
        })
    }
}

enum Event {
    Joined(u8, TcpStream),
    Msg(u8, Msg),
}

struct Peer {
    id: u8,
    out: Sender<Vec<u8>>,
    /// Kept to shut the socket down, which ends the reader thread.
    stream: TcpStream,
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

pub struct Net {
    pub id: u8,
    pub host: bool,
    peers: Vec<Peer>,
    events: Receiver<Event>,
    tx: Sender<Event>,
    /// Joined clients waiting for the world, taken by [`Net::joined`].
    joined: Vec<u8>,
}

fn reader(mut stream: TcpStream, id: u8, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let mut len = [0; 4];
        while stream.read_exact(&mut len).is_ok() {
            let mut frame = vec![0; u32::from_le_bytes(len) as usize];
            if stream.read_exact(&mut frame).is_err() {
                break;
            }
            let Some(m) = Msg::decode(&frame) else { break };
            if tx.send(Event::Msg(id, m)).is_err() {
                break;
            }
        }
        let _ = tx.send(Event::Msg(id, Msg::Leave { id }));
    });
}

fn writer(mut stream: TcpStream) -> Sender<Vec<u8>> {
    let (tx, rx) = channel::<Vec<u8>>();
    std::thread::spawn(move || {
        for frame in rx {
            if stream.write_all(&frame).is_err() {
                break;
            }
        }
    });
    tx
}

fn peer(stream: TcpStream, id: u8, tx: Sender<Event>) -> std::io::Result<Peer> {
    stream.set_nodelay(true)?;
    reader(stream.try_clone()?, id, tx);
    Ok(Peer { id, out: writer(stream.try_clone()?), stream })
}

impl Net {
    pub fn host(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let (tx, events) = channel();
        let accept = tx.clone();
        std::thread::spawn(move || {
            for (next, stream) in (1..=u8::MAX).zip(listener.incoming().flatten()) {
                if accept.send(Event::Joined(next, stream)).is_err() {
                    break;
                }
            }
        });
        Ok(Self { id: HOST_ID, host: true, peers: Vec::new(), events, tx, joined: Vec::new() })
    }

    /// Connects and waits for the world. Returns the client and the `.wld` bytes.
    pub fn join(addr: &str) -> std::io::Result<(Self, Vec<u8>)> {
        let stream = TcpStream::connect(addr)?;
        let (tx, events) = channel();
        let p = peer(stream, HOST_ID, tx.clone())?;
        let bad = || std::io::Error::other("host closed before sending the world");
        loop {
            match events.recv().map_err(|_| bad())? {
                Event::Msg(_, Msg::Welcome { id, world }) => {
                    return Ok((Self { id, host: false, peers: vec![p], events, tx, joined: Vec::new() }, world));
                }
                Event::Msg(_, Msg::Leave { .. }) => return Err(bad()),
                _ => {}
            }
        }
    }

    /// Messages received since the last call, by sender id.
    pub fn poll(&mut self) -> Vec<(u8, Msg)> {
        let mut msgs = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            match e {
                Event::Joined(id, stream) => {
                    // A failing socket shows up as a `Leave` from its reader.
                    if let Ok(p) = peer(stream, id, self.tx.clone()) {
                        self.peers.push(p);
                        self.joined.push(id);
                    }
                }
                Event::Msg(id, m) => {
                    if let Msg::Leave { .. } = m {
                        self.peers.retain(|p| p.id != id);
                    }
                    msgs.push((id, m));
                }
            }
        }
        msgs
    }

    /// Clients that connected since the last call and need a `Welcome`.
    pub fn joined(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.joined)
    }

    /// To one peer.
    pub fn send_to(&self, id: u8, m: &Msg) {
        if let Some(p) = self.peers.iter().find(|p| p.id == id) {
            let _ = p.out.send(m.encode());
        }
    }

    /// To every peer but `except`.
    pub fn send(&self, m: &Msg, except: Option<u8>) {
        let frame = m.encode();
        for p in self.peers.iter().filter(|p| Some(p.id) != except) {
            let _ = p.out.send(frame.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let msgs = [
            Msg::Welcome { id: 3, world: vec![1, 2, 3] },
            Msg::Player { id: 1, x: 1.5, y: -2.0, vx: 3.0, vy: 4.0, facing_left: true },
            Msg::Tile { x: 10, y: 20, tile: u16::MAX, fx: -1, fy: 36 },
            Msg::Liquid { x: 1, y: 2, amount: 200, kind: 1 },
            Msg::Mobs(vec![MobState { kind: 1, x: 5.0, y: 6.0, facing_left: false, hurt: true }]),
            Msg::Hit { index: 7, damage: 12 },
            Msg::Wall { x: 3, y: 4, wall: 0 },
            Msg::Leave { id: 2 },
            Msg::Time(0.5),
        ];
        for m in msgs {
            let f = m.encode();
            assert_eq!(u32::from_le_bytes(f[..4].try_into().unwrap()) as usize, f.len() - 4);
            assert_eq!(Msg::decode(&f[4..]), Some(m));
        }
        assert_eq!(Msg::decode(&[99]), None);
        assert_eq!(Msg::decode(&[2, 0]), None);
    }

    #[test]
    fn host_and_client_talk() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut host = Net::host(port).unwrap();
        let client = std::thread::spawn(move || Net::join(&format!("127.0.0.1:{port}")).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let id = loop {
            host.poll();
            if let Some(&id) = host.joined().first() {
                break id;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        host.send_to(id, &Msg::Welcome { id, world: vec![9; 100_000] });
        let (client, world) = client.join().unwrap();
        assert_eq!((client.id, world.len()), (1, 100_000));
        client.send(&Msg::Hit { index: 4, damage: 9 }, None);
        let got = loop {
            if let Some(m) = host.poll().into_iter().next() {
                break m;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(got, (1, Msg::Hit { index: 4, damage: 9 }));
        drop(client);
        let left = loop {
            if let Some(m) = host.poll().into_iter().next() {
                break m;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(left, (1, Msg::Leave { id: 1 }));
        assert!(host.peers.is_empty());
    }
}
