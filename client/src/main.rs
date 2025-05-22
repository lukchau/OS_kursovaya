use eframe::egui;
use std::sync::{Arc, Mutex};
use std::thread;
use std::net::{TcpStream, Shutdown};
use std::io::{Read, Write};
use std::fs::OpenOptions;
use serde_json::Value;
use ctrlc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::mpsc;
use chrono::DateTime;

struct ClientApp {
    server1_data: Arc<Mutex<String>>,
    server2_data: Arc<Mutex<String>>,
    server1_ip: String,
    server2_ip: String,
    status_message: Arc<Mutex<String>>,
    log_sender: mpsc::Sender<String>,
    connected_to_server1: bool,
    connected_to_server2: bool,
    server1_stop_sender: Option<mpsc::Sender<()>>,
    server2_stop_sender: Option<mpsc::Sender<()>>,
    server1_error: Arc<Mutex<bool>>,
    server2_error: Arc<Mutex<bool>>,
    server1_error_logged: Arc<Mutex<bool>>,
    server2_error_logged: Arc<Mutex<bool>>,
    client_id: u128,
}

// Реализация по умолчанию для ClientApp
impl Default for ClientApp {
    fn default() -> Self {
        let (log_sender, log_receiver) = mpsc::channel(); // Создание канала для логирования
        thread::spawn(move || logging_client(log_receiver)); // Запуск потока для логирования

        let client_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(); // Получение идентификатора клиента

        let log_sender_clone = log_sender.clone();
        log_sender_clone.send(format!("Клиент запущен. ID клиента: {}", client_id)).unwrap();

        Self {
            server1_data: Arc::new(Mutex::new("Нет данных".to_owned())),
            server2_data: Arc::new(Mutex::new("Нет данных".to_owned())),
            server1_ip: "127.0.0.1:7878".to_string(),
            server2_ip: "127.0.0.1:7879".to_string(),
            status_message: Arc::new(Mutex::new("Готов".to_string())),
            log_sender,
            connected_to_server1: false,
            connected_to_server2: false,
            server1_stop_sender: None,
            server2_stop_sender: None,
            server1_error: Arc::new(Mutex::new(false)),
            server2_error: Arc::new(Mutex::new(false)),
            server1_error_logged: Arc::new(Mutex::new(false)),
            server2_error_logged: Arc::new(Mutex::new(false)),
            client_id,
        }
    }
}

// Реализация интерфейса
impl eframe::App for ClientApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Курсовая работа (Вариант 6)"); // Заголовок окна

            ui.horizontal(|ui| {
                if ui.button("Подключиться к серверу 1").clicked() {
                    if !self.connected_to_server1 {
                        let ip = self.server1_ip.clone();
                        let data = Arc::clone(&self.server1_data);
                        let status = Arc::clone(&self.status_message);
                        let log_sender = self.log_sender.clone();
                        let server_name = "сервер 1".to_string();
                        let error_flag = Arc::clone(&self.server1_error);
                        let error_logged = Arc::clone(&self.server1_error_logged);

                        let (_handle, stop_sender) = get_server_data_async(
                            ip, data, status, log_sender, server_name, error_flag, error_logged, self.client_id
                        );
                        self.server1_stop_sender = Some(stop_sender); // Установка отправителя для остановки первого сервера
                        self.connected_to_server1 = true;
                    }
                }

                if ui.button("Подключиться к серверу 2").clicked() {
                    if !self.connected_to_server2 {
                        let ip = self.server2_ip.clone();
                        let data = Arc::clone(&self.server2_data);
                        let status = Arc::clone(&self.status_message);
                        let log_sender = self.log_sender.clone();
                        let server_name = "сервер 2".to_string();
                        let error_flag = Arc::clone(&self.server2_error);
                        let error_logged = Arc::clone(&self.server2_error_logged);

                        let (_handle, stop_sender) = get_server_data_async(
                            ip, data, status, log_sender, server_name, error_flag, error_logged, self.client_id
                        );
                        self.server2_stop_sender = Some(stop_sender); // Установка отправителя для остановки второго сервера
                        self.connected_to_server2 = true;
                    }
                }

                if ui.button("Отключиться от сервера 1").clicked() {
                    self.disconnect_from_server(1);
                }

                if ui.button("Отключиться от сервера 2").clicked() {
                    self.disconnect_from_server(2);
                }
            });

            ui.separator(); // Разделитель

            // Данные о серверах
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label("Сервер 1:");
                if *self.server1_error.lock().unwrap() {
                    ui.label("Сервер отключен или недоступен");
                } else {
                    ui.label(&*self.server1_data.lock().unwrap());
                }

                ui.label("Сервер 2:");
                if *self.server2_error.lock().unwrap() {
                    ui.label("Сервер отключен или недоступен");
                } else {
                    ui.label(&*self.server2_data.lock().unwrap());
                }
            });

            ui.separator();

            ui.label(format!("Статус: {}", *self.status_message.lock().unwrap()));
        });
    }
}

// Методы для приложения
impl ClientApp {
    fn disconnect_from_server(&mut self, server_number: u8) { // Отключение серверов
        match server_number {
            1 => {
                if self.connected_to_server1 {
                    if let Some(stop_sender) = self.server1_stop_sender.take() {
                        let _ = stop_sender.send(());
                    }
                    self.connected_to_server1 = false;
                    *self.server1_error.lock().unwrap() = false;
                    *self.server1_error_logged.lock().unwrap() = false;
                    *self.server1_data.lock().unwrap() = "Нет данных".to_string();
                    *self.status_message.lock().unwrap() = "Отключено от сервера 1".to_string();
                    self.log_sender.send(format!("Отключено от сервера 1. ID клиента: {}", self.client_id)).unwrap();
                }
            }
            2 => {
                if self.connected_to_server2 {
                    if let Some(stop_sender) = self.server2_stop_sender.take() {
                        let _ = stop_sender.send(());
                    }
                    self.connected_to_server2 = false;
                    *self.server2_error.lock().unwrap() = false;
                    *self.server2_error_logged.lock().unwrap() = false;
                    *self.server2_data.lock().unwrap() = "Нет данных".to_string();
                    *self.status_message.lock().unwrap() = "Отключено от сервера 2".to_string();
                    self.log_sender.send(format!("Отключено от сервера 2. ID клиента: {}", self.client_id)).unwrap();
                }
            }
            _ => {}
        }
    }
}

// Функции форматирования ответов от серверов из JSON в удобный формат
fn format_server1_response(json_str: &str) -> String {
    match serde_json::from_str::<Value>(json_str) {
        Ok(json) => {
            let buttons = json["mouse_buttons"].as_i64().unwrap_or(0);
            let has_wheel = json["has_scroll_wheel"].as_i64().unwrap_or(0) != 0;
            let timestamp = json["timestamp"].as_i64().unwrap_or(0);

            format!(
                "Количество кнопок мыши: {}\nНаличие колесика мыши: {}\nВремя получения данных: {}",
                buttons,
                if has_wheel { "да" } else { "нет" },
                DateTime::from_timestamp(timestamp, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "неизвестно".to_string())
            )
        }
        Err(e) => {
            format!("Ошибка парсинга данных: {}\nСырой ответ:\n{}", e, json_str)
        }
    }
}

fn format_server2_response(json_str: &str) -> String {
    match serde_json::from_str::<Value>(json_str) {
        Ok(json) => {
            let pid = json["pid"].as_u64().unwrap_or(0);
            let uptime_ms = json["uptime_ms"].as_u64().unwrap_or(0);
            let timestamp = json["timestamp"].as_i64().unwrap_or(0);

            let uptime_secs = uptime_ms / 1000;
            let hours = uptime_secs / 3600;
            let minutes = (uptime_secs % 3600) / 60;
            let seconds = uptime_secs % 60;

            format!(
                "ID процесса сервера: {}\nВремя работы сервера: {} ч {} мин {} сек\nВремя получения данных: {}",
                pid,
                hours,
                minutes,
                seconds,
                DateTime::from_timestamp(timestamp, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "неизвестно".to_string())
            )
        }
        Err(e) => {
            format!("Ошибка парсинга данных: {}\nСырой ответ:\n{}", e, json_str)
        }
    }
}

// Асинхронное получение данных от сервера
fn get_server_data_async(
    ip: String,
    data: Arc<Mutex<String>>,
    status: Arc<Mutex<String>>,
    log_sender: mpsc::Sender<String>,
    server_name: String,
    error_flag: Arc<Mutex<bool>>,
    error_logged: Arc<Mutex<bool>>,
    client_id: u128,
) -> (thread::JoinHandle<()>, mpsc::Sender<()>) {
    let (stop_sender, stop_receiver) = mpsc::channel(); // Создание канала для остановки

    let handle = thread::spawn(move || {
        log_sender.send(format!("Подключение к {}. ID клиента: {}", server_name, client_id)).unwrap();

        let mut stream = match TcpStream::connect(ip.as_str()) {
            Ok(stream) => {
                *error_flag.lock().unwrap() = false;
                stream
            }
            Err(e) => {
                *error_flag.lock().unwrap() = true;
                *data.lock().unwrap() = "Ошибка подключения".to_string();
                if !*error_logged.lock().unwrap() {
                    log_sender.send(format!("Ошибка подключения к {}. ID клиента: {}. Ошибка: {}", server_name, client_id, e)).unwrap();
                    *error_logged.lock().unwrap() = true;
                }
                return;
            }
        };

        loop {
            if stop_receiver.try_recv().is_ok() {
                let _ = stream.shutdown(Shutdown::Both); // Закрытие соединения
                return;
            }

            let result = {
                let request = "request"; // Запрос
                if let Err(e) = stream.write(request.as_bytes()) {
                    *error_flag.lock().unwrap() = true;
                    if !*error_logged.lock().unwrap() {
                        log_sender.send(format!("Ошибка отправки запроса к {}. ID клиента: {}. Ошибка: {}", server_name, client_id, e)).unwrap();
                        *error_logged.lock().unwrap() = true;
                    }
                    continue;
                }

                let mut buffer = [0; 1024]; // Буфер чтения данных
                match stream.read(&mut buffer) {
                    Ok(len) if len > 0 => {
                        let json_str = String::from_utf8_lossy(&buffer[..len]).to_string(); // Преобразование данных в строку
                        if !*error_logged.lock().unwrap() {
                            log_sender.send(format!("Полученная информация от {}. ID клиента: {}. Данные: {}", server_name, client_id, json_str)).unwrap();
                        }

                        if server_name == "сервер 1" {
                            format_server1_response(&json_str)
                        } else {
                            format_server2_response(&json_str)
                        }
                    }
                    Ok(_) => {
                        *error_flag.lock().unwrap() = true;
                        "Соединение закрыто сервером".to_string()
                    }
                    Err(e) => {
                        *error_flag.lock().unwrap() = true;
                        if !*error_logged.lock().unwrap() {
                            log_sender.send(format!("Ошибка чтения информации от {}. ID клиента: {}. Ошибка: {}", server_name, client_id, e)).unwrap();
                            *error_logged.lock().unwrap() = true;
                        }
                        format!("Ошибка чтения: {}", e)
                    }
                }
            };

            *data.lock().unwrap() = result.clone();
            *status.lock().unwrap() = format!(
                "Последнее действие: {}",
                if result.contains("Ошибка") { "Ошибка" } else { "Успех" }
            );

            thread::sleep(Duration::from_secs(10)); // Ожидание 10 секунд перед следующей отправкой данных
        }
    });

    (handle, stop_sender)
}

// Функция логгирования сообщений клиента
fn logging_client(receiver: mpsc::Receiver<String>) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("client_log.txt")
        .unwrap();

    for message in receiver {
        writeln!(file, "{}", message).unwrap();
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    let app = ClientApp::default();

    let log_sender = app.log_sender.clone();
    ctrlc::set_handler(move || {
        log_sender.send(format!("Клиент остановлен. ID клиента: {}", app.client_id)).unwrap();
        std::process::exit(0);
    }).expect("Ошибка установки обработчика Ctrl+C");

    eframe::run_native(
        "Курсовая работа (Вариант 6)", // Заголовок окна
        options,
        Box::new(|_| Ok(Box::new(app))),
    )
}
