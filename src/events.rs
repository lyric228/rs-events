use std::collections::HashMap;
use std::sync::{Arc, RwLock};
pub use std::any::Any;


type Handler = Box<dyn Fn(&[&dyn Any]) + Send + Sync + 'static>;

#[derive(Clone, Default)]
pub struct EventEmitter {
    pub events: Arc<RwLock<HashMap<String, Vec<Handler>>>>,
}

impl EventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on(&self, event: &str, listener: Handler) {
        let mut events = self.events
            .write()
            .unwrap();

        events.entry(event.to_string())
            .or_default()
            .push(listener);
    }

    pub fn emit(&self, event: &str, args: Vec<&dyn Any>) {
        let events = self.events
            .read()
            .unwrap();

        if let Some(handlers) = events.get(event) {
            for handler in handlers {
                handler(&args);
            }
        }
    }

    pub fn off(&self, event: &str) {
        let mut events = self.events
            .write()
            .unwrap();
        
        events.remove(event);
    }
}

#[macro_export]
macro_rules! on {
    ($emitter:expr, $event:expr, $($param:ident : $type:ty),*, $body:expr) => {{
        $emitter.on($event, Box::new(move |args| {
            let mut iter = args.iter();
            $(
                let $param = iter.next()
                    .expect(concat!("Not enough arguments for parameter: ", stringify!($param)))
                    .downcast_ref::<$type>()
                    .expect(concat!("Invalid type for parameter: ", stringify!($param)));
            )*
            if iter.next().is_some() {
                panic!("Too many arguments provided for event: {}", $event);
            }
            $body
        }));
    }};
}

#[macro_export]
macro_rules! emit {
    ($emitter:expr, $event:expr, $($arg:expr),*) => {{
        let args: Vec<&dyn Any> = vec![$(&$arg as &dyn Any),*];
        $emitter.emit($event, args);
    }};
}

#[macro_export]
macro_rules! off {
    ($emitter:expr, $event:expr) => {{
        $emitter.off($event);
    }};
}
