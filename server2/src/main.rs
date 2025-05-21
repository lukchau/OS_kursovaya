use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::sync::{Arc, Mutex, mpsc};
use std::fs::OpenOptions;
use chrono::Local;
use ctrlc;
use rayon::ThreadPoolBuilder;

struct ServerState {
    start_time: u128,
}

impl ServerState {
    fn new() -> Self {
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        ServerState { start_time }
    }
}

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<ServerState>>, log_sender: mpsc::Sender<String>) {
    let client_addr = stream.peer_addr().unwrap();
    log_sender.send(format!("Клиент подключен: {}", client_addr)).unwrap();

    let mut buffer = [0; 512];
    loop {
        match stream.read(&mut buffer) {
            Ok(len) => {
                if len == 0 {
                    log_sender.send(format!("Соединение с клиентом {} закрыто", client_addr)).unwrap();
                    return;
                }
                
                let request = String::from_utf8_lossy(&buffer[..len]).to_string();
                if request.trim() == "disconnect" {
                    log_sender.send(format!("Клиент отключился: {}", client_addr)).unwrap();
                    if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
                        log_sender.send(format!("Ошибка при отключении клиента {}: {}", client_addr, e)).unwrap();
                    }
                    return;
                }
            }
            Err(e) => {
                log_sender.send(format!("Ошибка чтения от клиента {}: {}", client_addr, e)).unwrap();
                if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
                    log_sender.send(format!("Ошибка при отключении клиента {}: {}", client_addr, e)).unwrap();
                }
                return;
            }
        }

        let pid = std::process::id();
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let start_time = state.lock().unwrap().start_time;
        let uptime_ms = current_time - start_time;

        let response = format!(
            "{{\"pid\": {}, \"uptime_ms\": {}, \"timestamp\": {}}}",
            pid,
            uptime_ms,
            Local::now().timestamp()
        );

        match stream.write(response.as_bytes()) {
            Ok(_) => {
                if let Err(e) = stream.flush() {
                    log_sender.send(format!("Ошибка сброса буфера для клиента {}: {}", client_addr, e)).unwrap();
                    return;
                }
                log_sender.send(format!("Данные отправлены клиенту {}: {}", client_addr, response)).unwrap();
            }
            Err(e) => {
                log_sender.send(format!("Ошибка отправки данных клиенту {}: {}", client_addr, e)).unwrap();
                return;
            }
        }

        thread::sleep(Duration::from_secs(10));
    }
}

fn logging_server(receiver: mpsc::Receiver<String>) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("server2_log.txt")
        .unwrap();

    for message in receiver {
        writeln!(file, "{}", message).unwrap();
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:7879").expect("Не удалось запустить сервер");
    println!("Сервер 2 запущен на порту 7879");

    let state = Arc::new(Mutex::new(ServerState::new()));
    let (log_sender, log_receiver) = mpsc::channel();
    let log_sender_clone = log_sender.clone();
    thread::spawn(move || logging_server(log_receiver));

    log_sender.send("Сервер запущен".to_string()).unwrap();

    ctrlc::set_handler(move || {
        log_sender_clone.send("Сервер остановлен".to_string()).unwrap();
        std::process::exit(0);
    }).expect("Ошибка при установке обработчика Ctrl+C");

    let pool = ThreadPoolBuilder::new().num_threads(5).build().unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                let log_sender = log_sender.clone();
                pool.spawn(move || handle_client(stream, state, log_sender));
            }
            Err(e) => {
                log_sender.send(format!("Ошибка подключения: {}", e)).unwrap();
                println!("Ошибка подключения: {}", e);
            }
        }
    }

    Ok(())
}