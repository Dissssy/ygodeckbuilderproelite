pub struct Buffer<T> {
    size: usize,
    buffer: Vec<Option<T>>,
}

impl<T> Buffer<T> {
    pub fn new(size: usize) -> Self {
        Self { size, buffer: Vec::new() }
    }
    pub fn get_index(&self) -> Option<usize> {
        let i = self.buffer.iter().position(|x| x.is_none());

        if let Some(i) = i {
            Some(i)
        } else if self.buffer.len() < self.size {
            Some(self.buffer.len() - 1)
        } else {
            None
        }
    }
    pub fn try_add(&mut self, item: T) -> Result<usize, T> {
        let index = self.buffer.iter().position(|x| x.is_none());
        if let Some(index) = index {
            self.buffer[index] = Some(item);
            Ok(index)
        } else if self.buffer.len() < self.size {
            self.buffer.push(Some(item));
            Ok(self.buffer.len() - 1)
        } else {
            Err(item)
        }
    }
    pub fn try_take(&mut self, index: usize) -> Option<T> {
        if index < self.buffer.len() {
            self.buffer[index].take()
        } else {
            None
        }
    }
    pub fn get_ref(&self, index: usize) -> Option<&T> {
        if index < self.buffer.len() {
            self.buffer[index].as_ref()
        } else {
            None
        }
    }
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
