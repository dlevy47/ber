use std::io::{self, Read};

pub struct TrackedRead<'a> {
    inner:      &'a mut (Read + 'a),
    read_bytes: usize,
}

impl<'a> TrackedRead<'a> {
    pub fn new (inner: &'a mut (Read + 'a)) -> TrackedRead<'a> {
        TrackedRead {
            inner: inner,
            read_bytes: 0,
        }
    }
    pub fn tell (&self) -> usize {
        self.read_bytes
    }
}

impl<'a> Read for TrackedRead<'a> {
    fn read (&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = try!(self.inner.read(buf));
        self.read_bytes += count;
        Ok(count)
    }
}
