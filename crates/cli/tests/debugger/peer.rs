//! A small IDE peer sends `run -i 1\0` and records the engine's length-prefixed reply.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(10);

pub(crate) fn clean(command: &mut Command) -> &mut Command {
    command.env_clear();
    for name in ["PATH", "HOME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

pub(crate) struct Peer {
    socket: BufReader<TcpStream>,
    child: Script,
}

impl Peer {
    pub(crate) fn launch(command: impl FnOnce(u16) -> Command) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let mut command = command(listener.local_addr()?.port());
        let mut child = Script(
            clean(&mut command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );

        let deadline = Instant::now() + WAIT;
        loop {
            match listener.accept() {
                Ok((socket, _)) => {
                    socket.set_nonblocking(false)?;
                    socket.set_read_timeout(Some(WAIT))?;
                    socket.set_write_timeout(Some(WAIT))?;
                    return Ok(Self {
                        socket: BufReader::new(socket),
                        child,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.0.try_wait()? {
                        return Err(io::Error::other(format!(
                            "debuggee exited before init: {status}"
                        )));
                    }
                    if Instant::now() >= deadline {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(crate) fn read(&mut self) -> io::Result<String> {
        let mut prefix = Vec::new();
        let bytes = self.socket.by_ref().take(20).read_until(0, &mut prefix)?;
        let length = (bytes > 1 && prefix.pop() == Some(0))
            .then(|| std::str::from_utf8(&prefix).ok()?.parse::<usize>().ok())
            .flatten()
            .filter(|size| *size <= 16 * 1024 * 1024)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid DBGp length"))?;
        let mut packet = vec![0; length];
        self.socket.read_exact(&mut packet)?;
        let mut terminator = [0];
        self.socket.read_exact(&mut terminator)?;
        if terminator != [0] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "missing DBGp terminator",
            ));
        }
        String::from_utf8(packet).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub(crate) fn send(&mut self, command: &str, id: usize) -> io::Result<()> {
        let wire = self.socket.get_mut();
        if let Some((command, payload)) = command.split_once(" -- ") {
            write!(wire, "{command} -i {id} -- {payload}\0")
        } else {
            write!(wire, "{command} -i {id}\0")
        }
    }

    pub(crate) fn collect(mut self, commands: &str) -> io::Result<(Vec<String>, Output)> {
        let mut packets = vec![self.read()?];
        for (index, command) in commands.lines().enumerate() {
            let id = index + 1;
            self.send(command, id)?;
            loop {
                let packet = self.read()?;
                let matches = packet.contains(&format!(r#"transaction_id="{id}""#));
                packets.push(packet);
                if matches {
                    break;
                }
            }
        }
        drop(self.socket);
        let output = self.child.finish()?;
        Ok((packets, output))
    }
}

struct Script(Child);

impl Script {
    fn finish(&mut self) -> io::Result<Output> {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.0.try_wait()? {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut stream) = self.0.stdout.take() {
                    stream.read_to_end(&mut stdout)?;
                }
                if let Some(mut stream) = self.0.stderr.take() {
                    stream.read_to_end(&mut stderr)?;
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
