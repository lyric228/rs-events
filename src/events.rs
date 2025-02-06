use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use lazy_static::lazy_static;
pub use std::any::Any;


lazy_static! {
    pub static ref EMITTERS: Mutex<HashMap<String, EventEmitter>> = Mutex::new(HashMap::new());
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

#[macro_export]
macro_rules! use_emitter {
    ($id:expr) => {
        $crate::CURRENT_EMITTER_ID.with(|cell| {
            *cell.borrow_mut() = String::from($id);
        });
    };
}

#[macro_export]
macro_rules! new_emitter {
    ($id:expr) => {
        $crate::EMITTERS.lock().unwrap().insert(
            String::from($id),
            $crate::EventEmitter::new()
        );
    };
}

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
