use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use lazy_static::lazy_static;
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

type Handler = Box<dyn Fn(&[&dyn Any]) + Send + Sync + 'static>;

#[derive(Clone, Default)]
pub struct EventEmitter {
    events: Arc<RwLock<HashMap<String, Vec<Handler>>>>,
}

impl EventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on<S: AsRef<str>>(&self, event: S, listener: Handler) {
        let mut events = self.events
            .write()
            .unwrap();
        let event = event.as_ref().to_string();

        if !events.contains_key(&event) {
            events.entry(event)
                .or_default()
                .push(listener);
        }
    }

    pub fn emit<S: AsRef<str>>(&self, event: S, args: Vec<&dyn Any>) {
        let events = self.events
            .read()
            .unwrap();

        if let Some(handlers) = events.get(event.as_ref()) {
            for handler in handlers {
                handler(&args);
            }
        }
    }

    pub fn off<S: AsRef<str>>(&self, event: S) {
        let mut events = self.events
            .write()
            .unwrap();
        
        events.remove(event.as_ref());
    }
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
        {
            let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
            let mut emitters = $crate::EMITTERS.lock().unwrap();

            if let Some(emitter) = emitters.get_mut(&emitter_id) {
                emitter.on($event, Box::new(move |_args: &[&dyn Any]| {
                    $body
                }));
            }
        }
    };
    
    ($event:expr, |$($arg:ident: $type:ty),*| $body:expr) => {
        {
            let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
            let mut emitters = $crate::EMITTERS.lock().unwrap();
            
            if let Some(emitter) = emitters.get_mut(&emitter_id) {
                emitter.on($event, Box::new(move |args: &[&dyn Any]| {
                    let mut iter = args.iter();
                    $(
                        let $arg = iter.next().unwrap().downcast_ref::<$type>()
                            .expect("Invalid argument type");
                    )*
                    $body
                }));
            }
        }
    };
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
    ($event:expr $(, $arg:expr)*) => {
        {
            let emitter_id = $crate::CURRENT_EMITTER_ID.with(|cell| cell.borrow().clone());
            let emitters = $crate::EMITTERS.lock().unwrap();

            if let Some(emitter) = emitters.get(&emitter_id) {
                let args = vec![$(&$arg as &dyn Any),*];
                emitter.emit($event, args);
            }
        }
    };
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
