//! Compare stream contexts and header APIs against a server with fixed wire responses.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::Path,
    thread,
    time::Duration,
};

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn stream_context_and_response_headers_match_php() {
    let Some(comparison) = Comparison::from_environment("stream-http") else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let redirect = format!("http://{}", destination.local_addr().unwrap());
    let (stop, task) = serve_in_thread(listener, redirect);
    let (stop_destination, destination_task) = serve_in_thread(destination, String::new());
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle_cases/context_http.php");
    let result = comparison.compare(&script, |_, command| {
        command.arg(format!("http://{address}"));
    });
    let _ = stop.send(());
    let _ = stop_destination.send(());
    task.join().unwrap();
    destination_task.join().unwrap();
    result.unwrap();
}

fn serve_in_thread(
    listener: TcpListener,
    redirect: String,
) -> (tokio::sync::oneshot::Sender<()>, thread::JoinHandle<()>) {
    listener.set_nonblocking(true).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (stop, closed) = tokio::sync::oneshot::channel();
    let task = thread::spawn(move || runtime.block_on(serve(listener, redirect, closed)));
    (stop, task)
}

async fn serve(
    listener: TcpListener,
    redirect: String,
    mut closed: tokio::sync::oneshot::Receiver<()>,
) {
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    loop {
        let socket = tokio::select! {
            _ = &mut closed => return,
            connection = listener.accept() => connection.unwrap().0,
        };
        let socket = socket.into_std().unwrap();
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = BufReader::new(socket);
        let mut first_line = String::new();
        if request.read_line(&mut first_line).is_err() {
            continue;
        }
        let mut first_line = first_line.split_ascii_whitespace();
        let (Some(method), Some(path)) = (first_line.next(), first_line.next()) else {
            continue;
        };
        let mut header = String::new();
        let mut host = String::new();
        let mut authorization = String::new();
        loop {
            header.clear();
            if !matches!(request.read_line(&mut header), Ok(length) if length > 0)
                || header == "\r\n"
            {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                if name.eq_ignore_ascii_case("Host") {
                    host = value.trim().into();
                }
                if name.eq_ignore_ascii_case("Authorization") {
                    authorization = value.trim().into();
                }
            }
        }
        let socket = request.get_mut();
        let received = format!("Host: {host}\nAuthorization: {authorization}");
        let cross_origin = format!("Location: {redirect}/received\r\n");
        let (status, extra, body) = match path {
            "/received" => ("200 Fine", "", received.as_str()),
            "/redirect-credentials" => ("302 Move", cross_origin.as_str(), ""),
            "/redirect" => ("302 Move", "Location: /ok\r\n", ""),
            "/missing" => ("404 Gone missing", "", "missing"),
            _ => (
                "200 Fine",
                "X-List: one\r\nx-list: two\r\nX-List: three\r\n",
                "body",
            ),
        };
        let _ = write!(
            socket,
            "HTTP/1.1 {status}\r\n{extra}X-Method: {method}\r\nContent-Length: {}\r\n\r\n",
            body.len(),
        );
        if method != "HEAD" {
            let _ = socket.write_all(body.as_bytes());
        }
    }
}
