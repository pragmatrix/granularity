use std::{cell::RefCell, rc::Rc};

struct Stream<T> {
    next: RefCell<Option<(T, Rc<Stream<T>>)>>,
}

impl<T> Stream<T> {
    fn end() -> Rc<Stream<T>> {
        Rc::new(Stream {
            next: RefCell::new(None),
        })
    }

    fn clone_next(&self) -> Option<(T, Rc<Stream<T>>)>
    where
        T: Clone,
    {
        let next = self.next.borrow();
        if let Some(vn) = &*next {
            return Some(vn.clone());
        }
        None
    }

    fn consume(self: &mut Rc<Self>) -> Option<T> {
        let mut next = self.next.borrow_mut();
        if let Some((value, next)) = &*next {
                    }
        None

    }
}

fn stream<T>() -> (Producer<T>, Consumer<T>) {
    let top = Stream::end();
    (
        Producer {
            top: top.clone(),
        },
        Consumer { next: top },
    )
}

pub struct Consumer<T> {
    next: Rc<Stream<T>>,
}

impl<T> Clone for Consumer<T> {
    fn clone(&self) -> Self {
        Consumer {
            next: self.next.clone(),
        }
    }
}

impl<T> Consumer<T> {
    pub fn take(&mut self) -> Option<T>
    where
        T: Clone,
    {
        if let Some((value, next)) = self.next.clone_next() {
            self.next = next;
            return Some(value);
        }
        None
    }
}

pub struct Producer<T> {
    top: Rc<Stream<T>>,
}

impl<T> Producer<T> {
    pub fn push(&mut self, value: T) {
        let mut next = self.top.next.borrow_mut();
        debug_assert!(next.is_none(), "Multiple producers are not supported");
        *next = Some((value, Stream::end()));
    }
}
