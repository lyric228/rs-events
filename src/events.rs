use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, RwLock};
use std::collections::HashMap;
use lazy_static::lazy_static;
use backtrace::Backtrace;

pub use std::sync::Arc;
pub use std::any::Any;


lazy_static! {
    pub static ref EMITTERS: Mutex<HashMap<String, EventEmitter>> = {
        let mut map = HashMap::new();
        map.insert(String::from("default"), EventEmitter::new());
        Mutex::new(map)
    };
}

thread_local! {
    pub static CURRENT_EMITTER_ID: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::from("default"));
}

type Handler = Arc<dyn Fn(&[&dyn Any]) + Send + Sync + 'static>;

#[derive(Debug)]
pub enum EmitError {
    NoListeners,
    LockError(String),
}

#[derive(Clone, Default)]
pub struct EventEmitter {
    pub events: Arc<RwLock<HashMap<String, Vec<Handler>>>>,
}

impl EventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on<S: AsRef<str>>(&self, event: S, listener: Handler) {
        let mut lock = self.events.write().unwrap_or_else(|e| e.into_inner());
        let event = event.as_ref().to_string();
        lock.entry(event).or_default().push(listener);
    }

    pub fn emit<S: AsRef<str>>(&self, event: S, args: Vec<&dyn Any>) -> Result<(), EmitError> {
        let lock = self.events.read().map_err(|e| {
            EmitError::LockError(format!("Failed to acquire read lock: {}", e))
        })?;
    
        let handlers = lock.get(event.as_ref())
            .cloned()
            .ok_or_else(|| {
                EmitError::NoListeners
            })?;
    
        if handlers.is_empty() {
            return Err(EmitError::NoListeners);
        }
    
        for handler in &handlers {
            let result = catch_unwind(AssertUnwindSafe(|| {
                handler(&args);
            }));
        
            if let Err(e) = result {
                let bt = Backtrace::new();
                let location = extract_user_location(&bt);
                eprintln!(
                    "emitter error 'Handler panicked: {:?}' at {}",
                    e,
                    location
                );
            }
        }
        Ok(())
    }

    pub fn off<S: AsRef<str>>(&self, event: S) {
        let mut lock = self.events.write().unwrap_or_else(|e| e.into_inner());
        lock.remove(event.as_ref());
    }
}


fn extract_user_location(bt: &Backtrace) -> String {
    let mut result = "unknown location".to_string();

    for frame in bt.frames() {
        backtrace::resolve(frame.ip(), |symbol| {
            if let (Some(file), Some(line)) = (symbol.filename(), symbol.lineno()) {
                let file_str = file.to_string_lossy();
                if file_str.contains("src/main.rs") {
                    result = format!("{}:{}", file.display(), line);
                }
            }
        });

        if result != "unknown location" {
            break;
        }
    }

    result
}


#[doc="
    Создаёт новый emitter с указанным идентификатором, если его ещё не существует.\n
    В отличие от use_emitter! не изменяет текущий emitter.\n\n
    Пример использования:\n
    ```rust
    new_emitter!(\"custom_emitter\");\n
    ```\n
    После создания можно переключиться на него с помощью use_emitter!
"]
#[macro_export]
macro_rules! new_emitter {
    ($id:expr) => {
        $crate::EMITTERS.lock().unwrap()
            .entry(String::from($id))
            .or_insert_with(|| $crate::EventEmitter::new());
    };
}

#[doc="
    Устанавливает текущий emitter по переданному идентификатору и возвращает его.\n
    Если указанный emitter не существует, он будет создан.\n\n
    Пример использования:\n
    ```rust
    use_emitter!(\"main_emitter\");\n
    // Все последующие вызовы on!/emit! будут использовать этот emitter\n
    ```\n
    Для возврата к default emitter:\n
    ```rust
    use_emitter!(\"default\");\n
    ```
"]
#[macro_export]
macro_rules! use_emitter {
    ($id:expr) => {{
        $crate::CURRENT_EMITTER_ID.with(|cell| {
            *cell.borrow_mut() = String::from($id);
        });
        
        $crate::EMITTERS.lock().unwrap()
            .entry(String::from($id))
            .or_insert_with(|| $crate::EventEmitter::new());
    }};
}

#[doc="
    Регистрирует обработчик для указанного события.\n
    Поддерживает два варианта использования:\n
    1. Без аргументов:\n
    ```rust
    on!(\"event_name\", || {
        println!(\"Event handled!\");
    });
    ```\n
    2. С аргументами определённых типов:\n
    ```rust
    on!(\"event_name\", |arg1: i32, arg2: String| {
        println!(\"Received {} and {}\", arg1, arg2);
    });
    ```\n
    Аргументы автоматически приводятся к указанным типам при вызове обработчика.
"]
#[macro_export]
macro_rules! on {
    ($event:expr, || $body:expr) => {
        $crate::on!($event, | | $body);
    };

    ($event:expr, |$($arg:ident: $type:ty),*| $body:expr) => {{
        let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
        let mut emitters = $crate::EMITTERS.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(emitter) = emitters.get_mut(&emitter_id) {
            let handler = std::sync::Arc::new(move |args: &[&dyn $crate::Any]| {
                let mut iter = args.iter();
                $(
                    let $arg = iter.next().unwrap_or_else(|| {
                        panic!(
                            "Ошибка: недостаточно аргументов для обработчика события \"{}\". Ожидался аргумент \"{}\" типа {}",
                            $event,
                            stringify!($arg),
                            stringify!($type)
                        )
                    })
                    .downcast_ref::<$type>()
                    .unwrap_or_else(|| {
                        panic!(
                            "Ошибка: аргумент \"{}\" для события \"{}\" имеет неверный тип. Ожидался тип {}",
                            stringify!($arg),
                            $event,
                            stringify!($type)
                        )
                    });
                )*
                $body
            });

            emitter.on($event, handler);
        }
    }};
}

#[doc="
    Вызывает событие с именем `event`, передавая ему аргументы.\n\n
    Примеры использования:\n
    Без аргументов:\n
    ```rust
    emit!(\"start_event\");\n
    ```\n
    С аргументами разных типов:\n
    ```rust
    let num = 42;\n
    let text = String::from(\"message\");\n
    emit!(\"data_event\", num, text);\n
    ```\n
    Аргументы должны реализовывать трейт Any.
"]
#[macro_export]
macro_rules! emit {
    ($event:expr $(, $arg:expr)*) => {{
        let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
        let emitters = $crate::EMITTERS.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(emitter) = emitters.get(&emitter_id) {
            let args = vec![$(&$arg as &dyn $crate::Any),*];
            if let Err(err) = emitter.emit($event, args) {
                eprintln!("Error during emitting event '{}': {:?}", $event, err);
            }
        } else {
            eprintln!("Emitter with id '{}' not found", emitter_id);
        }
    }};
}

#[doc="
    Удаляет все обработчики для указанного события.\n
    Пример использования:\n

    ```rust
    off!(\"event_name\");
    ```\n

    После вызова этого макроса событие перестанет обрабатываться.
"]
#[macro_export]
macro_rules! off {
    ($event:expr) => {
        {
            let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
            let mut emitters = $crate::EMITTERS.lock().unwrap();

            if let Some(emitter) = emitters.get_mut(&emitter_id) {
                emitter.off($event);
            }
        }
    };
}

#[doc="
    Регистрирует обработчик события, который сработает только один раз.\n\n
    Примеры использования:\n
    Без аргументов:\n
    ```rust
    once!(\"init\", || {
        println!(\"Инициализация выполнена!\");
    });
    ```\n
    С аргументами:\n
    ```rust
    once!(\"user_login\", |username: &str, id: i32| {
        println!(\"Привет {} (ID: {})!\", username, id);
    });
    ```\n
    После первого срабатывания обработчик автоматически удаляется.
"]
#[macro_export]
macro_rules! once {
    ($event:expr, || $body:expr) => {
        $crate::times!($event, 1, || $body);
    };
    
    ($event:expr, |$($arg:ident: $type:ty),*| $body:expr) => {
        $crate::times!($event, 1, |$($arg: $type),*| $body);
    };
}

#[doc="
    Регистрирует обработчик события, который сработает указанное количество раз.\n\n
    Примеры использования:\n
    Сработает 3 раза:\n
    ```rust
    times!(\"notification\", 3, || {
        println!(\"Уведомление!\");
    });
    ```\n
    С аргументами (5 срабатываний):\n
    ```rust
    times!(\"data_chunk\", 5, |chunk: &[u8]| {
        println!(\"Получен чанк {:?}\", chunk);
    });
    ```\n
    Срабатывает всегда кроме первого раза:\n
    ```rust
    times!(\"event\", 0, || {
        println!(\"Это не первый раз!\");
    });
    ```\n
    После исчерпания лимита обработчик автоматически удаляется.
"]
#[macro_export]
macro_rules! times {
    ($event:expr, $count:expr, || $body:expr) => {{
        let event_name = String::from($event);
        let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
        let event_clone = event_name.clone();
        
        {
            let mut emitters = $crate::EMITTERS.lock().unwrap();
            let emitter = emitters.get_mut(&emitter_id).unwrap().clone();
            let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new($count));
            let remaining_clone = remaining.clone();
            let weak_emitter = std::sync::Arc::downgrade(&emitter.events);
            
            emitter.on(&event_name, Arc::new(move |args: &[&dyn Any]| {
                let count = remaining_clone.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                if count >= 1 {
                    $body
                }
                
                if count == 1 {
                    if let Some(events) = weak_emitter.upgrade() {
                        if let Ok(mut guard) = events.write() {
                            guard.remove(&event_clone);
                        }
                    }
                }
            }));
        }
    }};
    
    ($event:expr, $count:expr, |$($arg:ident: $type:ty),*| $body:expr) => {{
        let event_name = String::from($event);
        let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
        let event_clone = event_name.clone();
        
        {
            let mut emitters = $crate::EMITTERS.lock().unwrap();
            let emitter = emitters.get_mut(&emitter_id).unwrap().clone();
            let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new($count));
            let remaining_clone = remaining.clone();
            let weak_emitter = std::sync::Arc::downgrade(&emitter.events);
            
            emitter.on(&event_name, Arc::new(move |args: &[&dyn Any]| {
                let count = remaining_clone.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                if count >= 1 {
                    let mut iter = args.iter();
                    $(
                        let $arg = iter.next().unwrap().downcast_ref::<$type>()
                            .expect("Неверный тип аргумента");
                    )*
                    $body
                }
                
                if count == 1 {
                    if let Some(events) = weak_emitter.upgrade() {
                        if let Ok(mut guard) = events.write() {
                            guard.remove(&event_clone);
                        }
                    }
                }
            }));
        }
    }};
}
