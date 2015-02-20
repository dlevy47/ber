use std::old_io::IoResult;

pub struct TrackedReader<'a> {
    inner:      &'a mut (Reader + 'a),
    read_bytes: usize,
}

impl<'a> TrackedReader<'a> {
    pub fn new (inner: &'a mut (Reader + 'a)) -> TrackedReader<'a> {
        TrackedReader {
            inner: inner,
            read_bytes: 0,
        }
    }
    pub fn tell (&self) -> usize {
        self.read_bytes
    }
}

impl<'a> Reader for TrackedReader<'a> {
    fn read (&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let count = try!(self.inner.read(buf));
        self.read_bytes += count;
        Ok(count)
    }
}
