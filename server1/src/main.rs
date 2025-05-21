use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::sync::mpsc;
use std::fs::OpenOptions;
use chrono::Local;
use winapi::um::winuser::{GetSystemMetrics, SM_CMOUSEBUTTONS, SM_MOUSEWHEELPRESENT};
use ctrlc;
use rayon::ThreadPoolBuilder;
use std::time::Duration;

fn handle_client(mut stream: TcpStream, log_sender: mpsc::Sender<String>) {
    let client_addr = stream.peer_addr().unwrap();
    log_sender.send(format!("Клиент подключен: {}", client_addr)).unwrap();

    let mut buffer = [0; 512];
    loop {
        // Проверяем, не отключился ли клиент
        match stream.read(&mut buffer) {
            Ok(len) if len > 0 => {
                let request = String::from_utf8_lossy(&buffer[..len]).to_string();
                if request.trim() == "disconnect" {
                    log_sender.send(format!("Клиент отключился: {}", client_addr)).unwrap();
                    if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
                        log_sender.send(format!("Ошибка при отключении клиента {}: {}", client_addr, e)).unwrap();
                    }
                    return;
                }
            }
            Ok(_) => {
                // Соединение было закрыто клиентом
                log_sender.send(format!("Соединение с клиентом {} закрыто", client_addr)).unwrap();
                return;
            }
            Err(e) => {
                log_sender.send(format!("Ошибка чтения от клиента {}: {}", client_addr, e)).unwrap();
                if let Err(e) = stream.shutdown(std::net::Shutdown::Both) {
                    log_sender.send(format!("Ошибка при отключении клиента {}: {}", client_addr, e)).unwrap();
                }
                return;
            }
        }

        // Получаем информацию о мыши
        let mouse_buttons = unsafe { GetSystemMetrics(SM_CMOUSEBUTTONS) };
        let has_scroll_wheel = unsafe { GetSystemMetrics(SM_MOUSEWHEELPRESENT) };

        let response = format!(
            "{{\"mouse_buttons\": {}, \"has_scroll_wheel\": {}, \"timestamp\": {}}}",
            mouse_buttons,
            has_scroll_wheel,
            Local::now().timestamp()
        );

        // Отправляем данные клиенту с проверкой соединения
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
        .open("server_log.txt")
        .unwrap();

    for message in receiver {
        writeln!(file, "{}", message).unwrap();
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:7878").expect("Не удалось запустить сервер");
    println!("Сервер 1 запущен на порту 7878");

    let (log_sender, log_receiver) = mpsc::channel();
    let log_sender_clone = log_sender.clone();
    thread::spawn(move || logging_server(log_receiver));

    log_sender.send("Сервер запущен".to_string()).unwrap();

    // Обработчик отключения через ctrol+c
    ctrlc::set_handler(move || {
        log_sender_clone.send("Сервер остановлен".to_string()).unwrap();
        std::process::exit(0);
    }).expect("Ошибка при установке обработчика Ctrl+C");

    let pool = ThreadPoolBuilder::new().num_threads(5).build().unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let log_sender = log_sender.clone();
                pool.spawn(move || handle_client(stream, log_sender));
            }
            Err(e) => {
                log_sender.send(format!("Ошибка подключения: {}", e)).unwrap();
                println!("Ошибка подключения: {}", e);
            }
        }
    }

    Ok(())
}