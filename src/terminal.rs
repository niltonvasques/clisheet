//! OSC 52 is write-only and requires explicit opt-in for the terminal in use.
use std::io::{self, Write};

/// Use the local desktop selection when available. Over SSH only the
/// receiving terminal can write the user's local clipboard.
pub fn copy_external(value: &str) -> io::Result<&'static str> {
    use std::env;
    use std::process::{Command, Stdio};

    let remote = ["SSH_TTY", "SSH_CONNECTION", "SSH_CLIENT"]
        .iter()
        .any(|key| env::var_os(key).is_some());
    if !remote {
        let helpers: &[(&str, &[&str], bool)] = &[
            (
                "wl-copy",
                &["--type", "text/plain;charset=utf-8"],
                env::var_os("WAYLAND_DISPLAY").is_some(),
            ),
            (
                "xclip",
                &["-selection", "clipboard", "-in"],
                env::var_os("DISPLAY").is_some(),
            ),
            (
                "xsel",
                &["--clipboard", "--input"],
                env::var_os("DISPLAY").is_some(),
            ),
            ("pbcopy", &[], cfg!(target_os = "macos")),
        ];
        for &(name, args, enabled) in helpers {
            if !enabled {
                continue;
            }
            let Ok(mut child) = Command::new(name)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            let written = child.stdin.take().unwrap().write_all(value.as_bytes());
            if child.wait()?.success() && written.is_ok() {
                return Ok(name);
            }
        }
    }
    copy_osc52(io::stdout(), value)?;
    Ok("OSC 52 enviado; depende da permissão do terminal")
}

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
