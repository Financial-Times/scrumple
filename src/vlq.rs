use std::str;

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub struct Vlq {
    buf: [u8; 13],
}

impl Vlq {
    pub fn new() -> Self {
        Self { buf: [0u8; 13] }
    }

    pub fn enc(&mut self, n: isize) -> &str {
        let sign = n < 0;
        let n = if sign { -n } else { n } as usize;
        let mut y = (n & 0xf) << 1 | sign as usize;
        let mut r = n >> 4;
        let mut l = 0;
        while r > 0 {
            y |= 0x20;
            self.buf[l] = B64[y];
            y = r & 0x1f;
            r >>= 5;
            l += 1;
        }
        self.buf[l] = B64[y];
        str::from_utf8(&self.buf[0..=l]).unwrap()
    }
}
