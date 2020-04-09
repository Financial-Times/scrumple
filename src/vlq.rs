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

mod test {
    #[test]
    fn test_vlq() {
        // 0000000000000000111111111111111122222222222222223333333333333333
        // 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
        // ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/
        let mut vlq = super::Vlq::new();
        assert_eq!(vlq.enc(0), "A");
        assert_eq!(vlq.enc(1), "C");
        assert_eq!(vlq.enc(-1), "D");
        assert_eq!(vlq.enc(5), "K");
        assert_eq!(vlq.enc(-5), "L");
        assert_eq!(vlq.enc(15), "e");
        assert_eq!(vlq.enc(-15), "f");
        assert_eq!(vlq.enc(16), "gB");
        assert_eq!(vlq.enc(1876), "o1D"); // 11 10101 0100
        assert_eq!(vlq.enc(-485223), "v2zd"); // 11101 10011 10110 0111
    }
}
