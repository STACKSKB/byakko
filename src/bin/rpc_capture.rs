use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const LISTEN_ADDR: &str = "127.0.0.1:3815";
const TARGET_ADDR: &str = "127.0.0.1:3814";
const CAPTURE_DIR: &str = "Research/captures";

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(0);

fn usage() {
    println!("Usage: rpc_capture [--help]");
    println!("Listen on {LISTEN_ADDR}, forward to {TARGET_ADDR}, and capture opaque TCP streams.");
}

fn next_connection_id() -> String {
    let sequence = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos}-{sequence}")
}

fn create_capture_files(connection_id: &str) -> io::Result<(File, File, PathBuf, PathBuf)> {
    let directory = Path::new(CAPTURE_DIR);
    fs::create_dir_all(directory)?;

    // create_new is intentional: an existing capture is never overwritten.
    let request_path = directory.join(format!("rpc-{connection_id}-request.bin"));
    let response_path = directory.join(format!("rpc-{connection_id}-response.bin"));
    let request_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request_path)?;
    let response_file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&response_path)
    {
        Ok(file) => file,
        Err(error) => {
            // Keep the already-created request capture. The proxy never deletes
            // captures, including when setup fails.
            return Err(io::Error::new(
                error.kind(),
                format!("cannot create {}: {error}", response_path.display()),
            ));
        }
    };

    Ok((request_file, response_file, request_path, response_path))
}

fn copy_and_capture(
    mut input: TcpStream,
    mut output: TcpStream,
    mut capture: File,
) -> io::Result<u64> {
    let mut buffer = [0_u8; 16 * 1024];
    let mut copied = 0_u64;

    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            capture.flush()?;
            output.shutdown(Shutdown::Write)?;
            return Ok(copied);
        }

        output.write_all(&buffer[..read])?;
        capture.write_all(&buffer[..read])?;
        capture.flush()?;
        copied += read as u64;
    }
}

fn capture_connection(client: TcpStream, connection_id: String) -> io::Result<()> {
    let target = TcpStream::connect(TARGET_ADDR)?;
    let (request_file, response_file, request_path, response_path) =
        create_capture_files(&connection_id)?;

    println!(
        "connection {connection_id}: capturing request={} response={}",
        request_path.display(),
        response_path.display()
    );
    io::stdout().flush()?;

    let request_input = client.try_clone()?;
    let request_output = target.try_clone()?;
    let response_input = target;
    let response_output = client;

    let request_thread =
        thread::spawn(move || copy_and_capture(request_input, request_output, request_file));
    let response_thread =
        thread::spawn(move || copy_and_capture(response_input, response_output, response_file));

    let request_result = request_thread
        .join()
        .map_err(|_| io::Error::other("request forwarding thread panicked"))?;
    let response_result = response_thread
        .join()
        .map_err(|_| io::Error::other("response forwarding thread panicked"))?;

    let request_bytes = request_result?;
    let response_bytes = response_result?;
    println!(
        "connection {connection_id}: captured {request_bytes} request bytes and {response_bytes} response bytes"
    );
    io::stdout().flush()?;
    Ok(())
}

fn main() -> io::Result<()> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        None => {}
        Some("--help") | Some("-h") if arguments.next().is_none() => {
            usage();
            return Ok(());
        }
        Some(argument) => {
            eprintln!("unknown argument: {argument}");
            usage();
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid arguments",
            ));
        }
    }

    let listener = TcpListener::bind(LISTEN_ADDR)?;
    println!("rpc_capture listening on {LISTEN_ADDR} and forwarding to {TARGET_ADDR}");
    io::stdout().flush()?;

    for connection in listener.incoming() {
        match connection {
            Ok(client) => {
                let connection_id = next_connection_id();
                thread::spawn(move || {
                    if let Err(error) = capture_connection(client, connection_id.clone()) {
                        eprintln!("connection {connection_id}: {error}");
                        let _ = io::stderr().flush();
                    }
                });
            }
            Err(error) => {
                eprintln!("accept failed: {error}");
                io::stderr().flush()?;
            }
        }
    }

    Ok(())
}
