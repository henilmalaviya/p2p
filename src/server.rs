use colored::Colorize;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Connection {
    socket: Arc<Mutex<TcpStream>>,
    addr: std::net::SocketAddr,
    nickname: String,
}

const CONNECTION_BUFFER_SIZE: usize = 1024;

async fn get_connection_by_addr(
    addr: std::net::SocketAddr,
    connections: Arc<Mutex<HashMap<std::net::SocketAddr, Connection>>>,
) -> Option<Arc<Connection>> {
    // get lock
    connections
        .lock()
        .await
        // from key
        .get(&addr)
        // clone connection into new Arc
        .map(|c| Arc::new(c.clone()))
}

async fn send_error_response(socket: Arc<Mutex<TcpStream>>, error: &str) {
    send_response(socket, format!("ERR {}", error).as_str(), true).await;
}

async fn send_response(socket: Arc<Mutex<TcpStream>>, response: &str, add_new_line: bool) {
    let mut locked_socket = socket.lock().await;

    let response = if add_new_line {
        format!("{}\n", response)
    } else {
        response.to_string()
    };

    locked_socket.write_all(response.as_bytes()).await.unwrap();

    locked_socket.flush().await.unwrap();
}

async fn handle_socket_registration(
    socket: Arc<Mutex<TcpStream>>,
    addr: std::net::SocketAddr,
    connections: Arc<Mutex<HashMap<std::net::SocketAddr, Connection>>>,
    nickname: String,
) {
    // check if socket has already registered
    {
        if connections.lock().await.contains_key(&addr) {
            send_error_response(socket.clone(), "ALR_REG").await;
            return;
        }
    }

    // check if nickname is already taken
    {
        if connections
            .lock()
            .await
            .values()
            .any(|c| c.nickname == nickname)
        {
            send_error_response(socket.clone(), "TKN").await;
            return;
        }
    }

    // create new kv pair
    connections.lock().await.insert(
        addr,
        Connection {
            socket: socket.clone(),
            addr,
            nickname: nickname.clone(),
        },
    );

    send_response(socket.clone(), "OK", true).await;

    println!(
        "{} {} {}",
        ">".bright_green(),
        nickname.bright_green().bold(),
        "Joined".bright_green()
    );
}

async fn handle_incoming_buffer(
    socket: Arc<Mutex<TcpStream>>,
    addr: std::net::SocketAddr,
    connections: Arc<Mutex<HashMap<std::net::SocketAddr, Connection>>>,
    data: &Vec<u8>,
) {
    // convert vector into string
    let data = String::from_utf8(data.to_vec()).unwrap();

    // split the string into words
    let mut data_splitted = data.split_whitespace();

    if data_splitted.clone().count() < 1 {
        send_error_response(socket.clone(), "NIL_CMD").await;
        return;
    }

    // get the first word
    let command = data_splitted.next().unwrap();

    match command {
        "REG" => {
            let nickname = data_splitted.clone().next().expect("NIL_NICK");

            handle_socket_registration(
                socket.clone(),
                addr,
                connections.clone(),
                nickname.to_string(),
            )
            .await;
        }

        // all other commands
        _ => {
            send_error_response(socket.clone(), "UNK_CMD").await;
        }
    }
}

async fn process_socket(
    socket: Arc<Mutex<TcpStream>>,
    addr: std::net::SocketAddr,
    connections: Arc<Mutex<HashMap<std::net::SocketAddr, Connection>>>,
) {
    // create data buffer
    let mut buffer = vec![0; CONNECTION_BUFFER_SIZE];

    loop {
        // try to read from socket
        let data_size = {
            // lock the socket
            let mut locked_socket = socket.lock().await;
            // read into buffer
            locked_socket.read(&mut buffer).await
        };

        match data_size {
            // close connection
            Ok(0) => {
                // try to get connection
                let conn = get_connection_by_addr(addr, connections.clone()).await;
                match conn {
                    None => {
                        // client did no register
                        // so no need to log anything
                    }
                    Some(conn) => {
                        // client had registered
                        println!(
                            "{} {} {}",
                            ">".bright_red(),
                            conn.nickname.bright_red().bold(),
                            "Left".bright_red()
                        );
                    }
                }
                break;
            }
            Ok(n) => {
                // extract out the n bytes from data buffer
                // and convert it into vector
                let data_buffer = buffer[..n].to_vec();

                handle_incoming_buffer(socket.clone(), addr, connections.clone(), &data_buffer)
                    .await
            }
            // failed to read
            Err(_e) => {
                break;
            }
        }
    }
}

pub async fn start_server() -> io::Result<()> {
    let addr = "127.0.0.1:4001";
    let listener = TcpListener::bind(addr).await?;

    let connections = Arc::new(Mutex::new(HashMap::new()));

    // for every incoming connection
    loop {
        // accept the connection
        let (socket, addr) = listener.accept().await?;

        let socket_arc = Arc::new(Mutex::new(socket));
        let connections_clone = connections.clone();

        // spawn new thread
        tokio::spawn(async move {
            process_socket(socket_arc, addr, connections_clone).await;
        });
    }
}
