//! OSC 52 is write-only and requires explicit opt-in for the terminal in use.
use std::io::{self, Write};

pub fn copy_osc52(mut out: impl Write, value: &str) -> io::Result<()> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if value.len() > 100_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Bloco excede o limite OSC 52. Use o clipboard interno",
        ));
    }
    let mut msg = Vec::with_capacity(9 + value.len().div_ceil(3) * 4);
    msg.extend_from_slice(b"\x1b]52;c;");
    for chunk in value.as_bytes().chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            msg.push(if i > chunk.len() {
                b'='
            } else {
                ALPHABET[((word >> shift) & 63) as usize]
            });
        }
    }
    msg.push(7);
    out.write_all(&msg)?;
    out.flush()
}
#[test]
fn unicode_clipboard() {
    let mut v = vec![];
    copy_osc52(&mut v, "Café").unwrap();
    assert_eq!(&v, b"\x1b]52;c;Q2Fmw6k=\x07");
}
