use std::{collections::HashSet, hash::Hash, rc::Rc};

pub struct Store<T> {
    values: HashSet<Rc<T>>,
}

impl<T> Store<T>
where
    T: Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            values: HashSet::new(),
        }
    }

    pub fn intern(&mut self, value: T) -> Rc<T> {
        if let Some(existing) = self.values.get(&value) {
            return Rc::clone(existing);
        }

        let value = Rc::new(value);
        self.values.insert(Rc::clone(&value));
        value
    }
}
