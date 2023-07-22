use std::{cell::RefCell, iter, rc::Rc};

use crate::{stream, Value};

/// A producer value. Use produce() to produce new values, and subscribe(), to subscribe to a
/// produce and return a consumer. The consumer receivers all new values that are produced by the
/// producer.
pub type Producer<T> = Value<stream::Producer<T>>;
// TODO: There should be Value that preserves its value on invalidation, and the compute function
// should take the old value (if existing). This way we could remove the reference counter here.
pub type Consumer<T> = Value<ConsumerValue<T>>;

impl<T> Producer<T> {
    pub fn subscribe(&self) -> Consumer<T> {
        let producer = self.get_ref();
        let consumer = ConsumerValue::new(producer.subscribe());
        let producer = self.clone();
        self.runtime().computed(move || {
            producer.track();
            consumer.clone()
        })
    }

    pub fn produce(&mut self, value: T) {
        self.apply(|mut p| {
            p.produce(value);
            p
        })
    }
}

pub struct ConsumerValue<T>(Rc<RefCell<stream::Consumer<T>>>);

impl<T> Clone for ConsumerValue<T> {
    fn clone(&self) -> Self {
        ConsumerValue(self.0.clone())
    }
}

impl<T> ConsumerValue<T> {
    pub fn new(consumer: stream::Consumer<T>) -> Self {
        ConsumerValue(Rc::new(RefCell::new(consumer)))
    }

    pub fn drain(&self) -> impl Iterator<Item = T> + '_
    where
        T: Clone,
    {
        let mut consumer = self.0.borrow_mut();
        iter::from_fn(move || consumer.drain_one())
    }
}
