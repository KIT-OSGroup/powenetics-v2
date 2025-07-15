use std::{
    io,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    ops::DerefMut,
    sync::Mutex,
};

use anyhow::bail;
use async_broadcast::{broadcast, Receiver, RecvError, Sender};
use powenetics_v2::{Powenetics, PoweneticsData, PoweneticsSubscriber};
use smol::Async;

use super::{CsvError, CsvSubscriber};

struct VecCell(Mutex<Vec<u8>>);

impl std::io::Write for &VecCell {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

async fn csv_streamer(
    mut stream: Async<TcpStream>,
    mut recv: Receiver<PoweneticsData>,
) -> anyhow::Result<()> {
    use smol::io::AsyncWriteExt;

    let buf: VecCell = VecCell(Mutex::new(Vec::new()));
    let mut sub = CsvSubscriber {
        csv: csv::Writer::from_writer(&buf),
    };
    // unwrap: writing to Vec cannot fail
    sub.write_header().unwrap();
    loop {
        let vec = std::mem::take(buf.0.lock().unwrap().deref_mut());
        stream.write_all(&vec).await?;
        let item = match recv.recv().await {
            Ok(item) => item,
            Err(RecvError::Closed) => bail!("channel closed"),
            Err(RecvError::Overflowed(_)) => continue,
        };
        // unwrap: writing to Vec cannot fail
        sub.update(&item).unwrap();
    }
}

async fn csv_server(listener: TcpListener, recv: Receiver<PoweneticsData>) -> io::Result<()> {
    let listener = Async::new(listener)?;
    let recv = recv.deactivate();
    loop {
        let (stream, _peer_addr) = listener.accept().await?;
        smol::spawn(csv_streamer(stream, recv.activate_cloned())).detach();
    }
}

struct ServerSubscriber {
    send: Sender<PoweneticsData>,
}

impl PoweneticsSubscriber for ServerSubscriber {
    fn update(&mut self, p: &PoweneticsData) -> anyhow::Result<bool> {
        // Ignore SendError: not an error if nobody is receiving
        let _ = self.send.broadcast_blocking(p.clone());
        Ok(false)
    }
}

pub fn subscribe_csv_server<A: ToSocketAddrs>(p: &mut Powenetics, addr: A) -> Result<(), CsvError> {
    let listener = TcpListener::bind(addr)?;
    let (mut send, mut recv) = broadcast::<PoweneticsData>(1);
    recv.set_overflow(true);
    send.set_await_active(false);
    smol::spawn(csv_server(listener, recv)).detach();
    p.subscribe(Box::new(ServerSubscriber { send }));
    Ok(())
}
