//! Multiplayer over TCP with Terraria's packets (see `proto`). The host owns
//! the world and the monsters: a joining client goes through the usual
//! greeting, gets the world in sections, then players, edits and monsters are
//! kept in step.
//!
//! Each connection has a reader thread feeding one inbox and a writer thread
//! fed by a channel, so streaming a large world never blocks the frame.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use formats::wld::Tile;
use world::World;

use crate::proto::{self, Packet};

pub const DEFAULT_PORT: u16 = 7777;
/// Host's own player slot; clients get 1, 2...
pub const HOST_ID: u8 = 0;
const JOIN_TIMEOUT: Duration = Duration::from_secs(30);
/// Monster slots a client keeps track of.
const MAX_MOBS: usize = 1000;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// Host side only: the world (`.wld` bytes) for a client that asked for it.
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
    /// The whole tile after an edit: block, frame, wall, liquid.
    Tile {
        x: u32,
        y: u32,
        cell: Tile,
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
    pub hp: i32,
}

enum Event {
    Joined(u8, TcpStream),
    Frame(u8, Vec<u8>),
    Closed(u8),
}

struct Peer {
    id: u8,
    out: Sender<Vec<u8>>,
    /// Kept to shut the socket down, which ends the reader thread.
    stream: TcpStream,
    /// The world handed over with `Welcome`, streamed when tiles are asked for.
    world: Option<Arc<Vec<u8>>>,
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
    /// Clients that asked for the world, taken by [`Net::joined`].
    joined: Vec<u8>,
    important: Vec<bool>,
    /// Host: monster slots used by the last snapshot sent.
    mob_slots: usize,
    /// Client: monster slots as received, and whether they changed.
    mobs: Vec<Option<MobState>>,
    mobs_changed: bool,
    /// Messages that came during the greeting.
    backlog: Vec<(u8, Msg)>,
}

fn reader(mut stream: TcpStream, id: u8, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let mut len = [0; 2];
        while stream.read_exact(&mut len).is_ok() {
            let Some(n) = (u16::from_le_bytes(len) as usize).checked_sub(2) else { break };
            let mut frame = vec![0; n];
            if stream.read_exact(&mut frame).is_err() || tx.send(Event::Frame(id, frame)).is_err() {
                break;
            }
        }
        let _ = tx.send(Event::Closed(id));
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
    Ok(Peer { id, out: writer(stream.try_clone()?), stream, world: None })
}

impl Net {
    fn new(host: bool, events: Receiver<Event>, tx: Sender<Event>) -> Self {
        Self {
            id: HOST_ID,
            host,
            peers: Vec::new(),
            events,
            tx,
            joined: Vec::new(),
            important: proto::important_table(),
            mob_slots: 0,
            mobs: Vec::new(),
            mobs_changed: false,
            backlog: Vec::new(),
        }
    }

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
        Ok(Self::new(true, events, tx))
    }

    /// Connects, greets the server and receives the whole world.
    pub fn join(addr: &str) -> std::io::Result<(Self, World)> {
        let stream = TcpStream::connect(addr)?;
        let (tx, events) = channel();
        let p = peer(stream, HOST_ID, tx.clone())?;
        let _ = p.out.send(proto::connect());
        let mut net = Self::new(false, events, tx);
        let err = |s: &str| std::io::Error::other(s.to_string());
        let mut world: Option<World> = None;
        loop {
            let frame = match net.events.recv_timeout(JOIN_TIMEOUT) {
                Ok(Event::Frame(_, f)) => f,
                Ok(Event::Closed(_)) => return Err(err("the server closed the connection")),
                Ok(Event::Joined(..)) => continue,
                Err(_) => return Err(err("no answer from the server")),
            };
            match proto::decode(&frame, &net.important) {
                Some(Packet::Slot(id)) => {
                    net.id = id;
                    for f in proto::greeting(id) {
                        let _ = p.out.send(f);
                    }
                }
                Some(Packet::Disconnect(reason)) => return Err(err(&format!("refused: {reason}"))),
                Some(Packet::WorldInfo(info)) if world.is_none() => {
                    let empty =
                        formats::synth::write_world(&info.name, info.width, info.height, &net.important, |_, _| {
                            Tile::default()
                        });
                    world = Some(World::parse(&empty).map_err(|e| err(&e.to_string()))?);
                    net.backlog.push((HOST_ID, Msg::Time(info.time)));
                    let _ = p.out.send(proto::request_tiles());
                }
                Some(Packet::Section { x, y, w, h, tiles }) => {
                    let Some(world) = world.as_mut() else { continue };
                    for (i, t) in tiles.iter().enumerate() {
                        let (tx, ty) = (x + i as u32 % w, y + i as u32 / w);
                        if i as u32 / w < h && world.in_bounds(tx as i64, ty as i64) {
                            world.put_cell(tx, ty, t);
                        }
                    }
                }
                Some(Packet::Complete) => {
                    let Some(world) = world else { return Err(err("tiles came before the world info")) };
                    let _ = p.out.send(proto::spawn(net.id));
                    net.peers.push(p);
                    return Ok((net, world));
                }
                Some(packet) => {
                    let mut msgs = std::mem::take(&mut net.backlog);
                    net.client_packet(packet, &mut msgs);
                    net.backlog = msgs;
                }
                None => {}
            }
        }
    }

    fn client_packet(&mut self, packet: Packet, msgs: &mut Vec<(u8, Msg)>) {
        match packet {
            Packet::Msg(m) => msgs.push((HOST_ID, m)),
            Packet::Npc(i, s) if (i as usize) < MAX_MOBS => {
                let i = i as usize;
                if self.mobs.len() <= i {
                    self.mobs.resize(i + 1, None);
                }
                self.mobs[i] = s;
                self.mobs_changed = true;
            }
            _ => {}
        }
    }

    fn host_packet(&mut self, from: u8, packet: Packet, msgs: &mut Vec<(u8, Msg)>) {
        let Some(p) = self.peers.iter().find(|p| p.id == from) else { return };
        match packet {
            Packet::Connect(v) if v == format!("Terraria{}", proto::RELEASE) => {
                let _ = p.out.send(proto::slot(from));
            }
            Packet::Connect(_) => {
                let _ = p.out.send(proto::disconnect("You are not using the same version as this server."));
            }
            Packet::RequestWorld => self.joined.push(from),
            Packet::RequestTiles => {
                if let Some(wld) = p.world.clone() {
                    let out = p.out.clone();
                    std::thread::spawn(move || {
                        let Ok(world) = World::parse(&wld) else { return };
                        for f in proto::world_frames(&world) {
                            if out.send(f).is_err() {
                                break;
                            }
                        }
                    });
                }
            }
            // Players speak only for themselves.
            Packet::Msg(Msg::Player { x, y, vx, vy, facing_left, .. }) => {
                msgs.push((from, Msg::Player { id: from, x, y, vx, vy, facing_left }))
            }
            Packet::Msg(Msg::Leave { .. }) => msgs.push((from, Msg::Leave { id: from })),
            Packet::Msg(Msg::Mobs(_) | Msg::Time(_) | Msg::Welcome { .. }) => {}
            Packet::Msg(m) => msgs.push((from, m)),
            _ => {}
        }
    }

    /// Messages received since the last call, by sender id.
    pub fn poll(&mut self) -> Vec<(u8, Msg)> {
        let mut msgs = std::mem::take(&mut self.backlog);
        while let Ok(e) = self.events.try_recv() {
            match e {
                Event::Joined(id, stream) => {
                    // A failing socket shows up as `Closed` from its reader.
                    if let Ok(p) = peer(stream, id, self.tx.clone()) {
                        self.peers.push(p);
                    }
                }
                Event::Frame(id, frame) => {
                    let Some(packet) = proto::decode(&frame, &self.important) else { continue };
                    if self.host {
                        self.host_packet(id, packet, &mut msgs);
                    } else {
                        self.client_packet(packet, &mut msgs);
                    }
                }
                Event::Closed(id) => {
                    self.peers.retain(|p| p.id != id);
                    msgs.push((id, Msg::Leave { id }));
                }
            }
        }
        if std::mem::take(&mut self.mobs_changed) {
            msgs.push((HOST_ID, Msg::Mobs(self.mobs.iter().flatten().copied().collect())));
        }
        msgs
    }

    /// Clients that asked for the world since the last call and need a `Welcome`.
    pub fn joined(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.joined)
    }

    fn frames(&mut self, m: &Msg) -> Vec<Vec<u8>> {
        let mut frames = proto::encode(m);
        if let Msg::Mobs(mobs) = m {
            frames.extend((mobs.len()..self.mob_slots).map(|i| proto::npc(i as u16, None)));
            self.mob_slots = mobs.len();
        }
        frames
    }

    /// To one peer. `Welcome` sends the world info and keeps the world for the sections.
    pub fn send_to(&mut self, id: u8, m: &Msg) {
        let frames = self.frames(m);
        let Some(p) = self.peers.iter_mut().find(|p| p.id == id) else { return };
        if let Msg::Welcome { world, .. } = m {
            let Ok(header) = formats::wld::Header::parse(world) else { return };
            let _ = p.out.send(proto::world_info(&header, 0.0));
            p.world = Some(Arc::new(world.clone()));
        }
        for f in frames {
            let _ = p.out.send(f);
        }
    }

    /// To every peer but `except`.
    pub fn send(&mut self, m: &Msg, except: Option<u8>) {
        let frames = self.frames(m);
        for p in self.peers.iter().filter(|p| Some(p.id) != except) {
            for f in &frames {
                let _ = p.out.send(f.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::wld::NO_TILE;
    use std::time::Instant;

    fn wait<T>(mut f: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(v) = f() {
                return v;
            }
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn host_and_client_talk() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut host = Net::host(port).unwrap();
        let client = std::thread::spawn(move || Net::join(&format!("127.0.0.1:{port}")).unwrap());
        let wld = formats::synth::write_world("w", 420, 200, &proto::important_table(), |x, y| Tile {
            tile: if y > 120 + x % 5 { 1 } else { NO_TILE },
            wall: (y > 100) as u16 * 2,
            ..Tile::default()
        });
        let id = wait(|| {
            host.poll();
            host.joined().first().copied()
        });
        host.send_to(id, &Msg::Welcome { id, world: wld.clone() });
        wait(|| {
            host.poll();
            client.is_finished().then_some(())
        });
        let (mut client, world) = client.join().unwrap();
        assert_eq!(client.id, 1);
        assert_eq!(world.to_wld(), World::parse(&wld).unwrap().to_wld(), "same tiles");

        client.send(&Msg::Hit { index: 4, damage: 9 }, None);
        let got = wait(|| host.poll().into_iter().find(|(_, m)| matches!(m, Msg::Hit { .. })));
        assert_eq!(got, (1, Msg::Hit { index: 4, damage: 9 }));

        let s = MobState { kind: 0, x: 1.0, y: 2.0, facing_left: true, hurt: false, hp: 20 };
        host.send(&Msg::Mobs(vec![s, s]), None);
        host.send(&Msg::Mobs(vec![s]), None);
        let mobs = wait(|| {
            let mut last = client.poll().into_iter().filter_map(|(_, m)| match m {
                Msg::Mobs(v) => Some(v),
                _ => None,
            });
            last.next_back().filter(|v| v.len() == 1)
        });
        assert_eq!(mobs, vec![s]);

        drop(client);
        let left = wait(|| host.poll().into_iter().find(|(_, m)| matches!(m, Msg::Leave { .. })));
        assert_eq!(left, (1, Msg::Leave { id: 1 }));
        assert!(host.peers.is_empty());
    }

    #[test]
    fn wrong_version_is_refused() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut host = Net::host(port).unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(&proto::W::new(proto::id::CONNECT).str("Terraria1").done()).unwrap();
        let mut len = [0; 2];
        let mut reply = Vec::new();
        wait(|| {
            host.poll();
            s.set_read_timeout(Some(Duration::from_millis(10))).unwrap();
            s.read_exact(&mut len).ok()?;
            reply = vec![0; u16::from_le_bytes(len) as usize - 2];
            s.read_exact(&mut reply).ok()
        });
        assert!(matches!(proto::decode(&reply, &[]), Some(Packet::Disconnect(_))));
    }
}
