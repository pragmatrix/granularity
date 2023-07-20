use std::{cell::RefCell, iter, mem, rc::Rc};
struct Element<T> {
    next: RefCell<Option<(T, Rc<Element<T>>)>>,
}

impl<T> Element<T> {
    fn end() -> Rc<Element<T>> {
        Rc::new(Element::default())
    }

    fn clone_value_and_next(&self) -> Option<(T, Rc<Element<T>>)>
    where
        T: Clone,
    {
        let next = self.next.borrow();
        if let Some(vn) = &*next {
            return Some(vn.clone());
        }
        None
    }

    fn into_inner(self) -> Option<(T, Rc<Element<T>>)> {
        self.next.into_inner()
    }
}

impl<T> Default for Element<T> {
    fn default() -> Self {
        Element {
            next: RefCell::new(None),
        }
    }
}

pub fn stream<T>() -> (Producer<T>, Consumer<T>) {
    let top = Element::end();
    (Producer { top: top.clone() }, Consumer { next: top })
}

/// A producer points to the consuming end element of the stream.
pub struct Producer<T> {
    top: Rc<Element<T>>,
}

impl<T> Producer<T> {
    pub fn produce(&mut self, value: T) {
        let new_end = Element::end();
        {
            let mut next = self.top.next.borrow_mut();
            debug_assert!(next.is_none(), "Multiple producers are not supported");
            *next = Some((value, new_end.clone()));
        }
        self.top = new_end;
    }
}

pub struct Consumer<T> {
    next: Rc<Element<T>>,
}

/// Custom implementation of Clone for Consumer<T> to avoid putting a Clone requirement on T.
impl<T> Clone for Consumer<T> {
    fn clone(&self) -> Self {
        Consumer {
            next: self.next.clone(),
        }
    }
}

impl<T> Consumer<T> {
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_
    where
        T: Clone,
    {
        iter::from_fn(|| self.drain_one())
    }

    pub fn drain_one(&mut self) -> Option<T>
    where
        T: Clone,
    {
        if let Some(next) = Rc::get_mut(&mut self.next) {
            // We are the only owner of next, so we can consume it.
            if let Some((value, next)) = mem::take(next).into_inner() {
                self.next = next;
                return Some(value);
            }
            // Consumed an end, which means that there were no producers anymore.
            return None;
        }

        if let Some((value, next)) = self.next.clone_value_and_next() {
            self.next = next;
            return Some(value);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stream() {
        let (mut producer, mut consumer) = stream();
        assert_eq!(consumer.drain().collect::<Vec<_>>(), []);
        producer.produce(1);
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn test_stream() {
        let (mut producer, mut consumer) = stream();
        producer.produce(1);
        producer.produce(2);
        producer.produce(3);
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn test_stream_clone() {
        let (mut producer, mut consumer) = stream();
        producer.produce(1);
        producer.produce(2);
        producer.produce(3);
        let mut consumer2 = consumer.clone();
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(consumer2.drain().collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn clone_after_drain() {
        let (mut producer, mut consumer) = stream();
        producer.produce(1);
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [1]);
        let mut consumer2 = consumer.clone();
        producer.produce(2);
        assert_eq!(consumer2.drain().collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn test_stream_clone_in_flight() {
        let (mut producer, mut consumer) = stream();
        producer.produce(1);
        let mut consumer2 = consumer.clone();
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [1]);
        producer.produce(2);
        assert_eq!(consumer2.drain().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn test_production_after_drain() {
        let (mut producer, mut consumer) = stream();
        producer.produce(1);
        producer.produce(2);
        producer.produce(3);
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [1, 2, 3]);
        producer.produce(4);
        assert_eq!(consumer.drain().collect::<Vec<_>>(), [4]);
    }
}
