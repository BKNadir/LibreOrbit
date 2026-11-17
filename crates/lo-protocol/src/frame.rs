//! TCP framing and the official server's downstream compression.

use crate::VERSION_REQUEST_ID;

/// Largest frame accepted from a peer (a 3-byte length allows 16 MiB; the first client frame has
/// a 4-byte length, which `protocol.py` does not bound).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

const MAGIC_BLOCK: u32 = 0x444F_4233; // "DOB3"
const MAGIC_LEGACY: u32 = 0x444F_4C34; // "DOL4"

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("frame too large ({0} bytes)")]
    TooLarge(usize),
    #[error("LZ4: {0}")]
    Lz4(&'static str),
    #[error("LZ4: length does not match ({got} != {want})")]
    Lz4Length { got: usize, want: usize },
    #[error("compressed block CRC error")]
    Crc,
}

/// Server -> client frame: 3-byte BE length + payload.
pub fn frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() >= 1 << 24 {
        return Err(FrameError::TooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(payload.len() + 3);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes()[1..]);
    out.extend_from_slice(payload);
    Ok(out)
}

/// Splits the byte stream of one TCP direction into message payloads.
#[derive(Debug)]
pub struct Framer {
    client_side: bool,
    buf: Vec<u8>,
    first: bool,
}

impl Framer {
    /// `client_side`: the stream the client sends (its first frame, VersionRequest, has a 4-byte length).
    pub fn new(client_side: bool) -> Self {
        Framer {
            client_side,
            buf: Vec::new(),
            first: true,
        }
    }

    /// Adds received bytes and returns the complete payloads.
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, FrameError> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        let mut start = 0;
        loop {
            let b = &self.buf[start..];
            let vr = VERSION_REQUEST_ID.to_be_bytes();
            let hdr =
                if self.client_side && self.first && b.len() >= 6 && b[0] == 0 && b[4..6] == vr {
                    4
                } else {
                    3
                };
            if b.len() < hdr {
                break;
            }
            let n = b[..hdr].iter().fold(0usize, |a, &x| (a << 8) | x as usize);
            if n > MAX_FRAME {
                return Err(FrameError::TooLarge(n));
            }
            if b.len() < hdr + n {
                break;
            }
            out.push(b[hdr..hdr + n].to_vec());
            start += hdr + n;
            self.first = false;
        }
        self.buf.drain(..start);
        Ok(out)
    }

    /// Bytes received but not yet part of a complete frame.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

/// The compression a downstream payload used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// "DOB3": the data holds inner frames with a 3-byte length.
    Block,
    /// "DOL4": the data is a single message.
    Legacy,
}

/// A message payload and the compression it came in.
pub type Part = (Vec<u8>, Option<Compression>);

/// Decompresses an official-server payload if it is compressed (`FF FF` + magic + raw length +
/// CRC32 + LZ4 block) -> the message payloads it holds.
pub fn downstream(payload: &[u8]) -> Result<Vec<Part>, FrameError> {
    if payload.len() >= 14 && payload[0] == 0xFF && payload[1] == 0xFF {
        let word =
            |i: usize| u32::from_be_bytes(payload[i..i + 4].try_into().expect("length checked"));
        let (magic, raw_len, crc) = (word(2), word(6) as usize, word(10));
        if magic == MAGIC_BLOCK || magic == MAGIC_LEGACY {
            if raw_len > MAX_FRAME {
                return Err(FrameError::TooLarge(raw_len));
            }
            let data = lz4_block(&payload[14..], raw_len)?;
            if crc32fast::hash(&data) != crc {
                return Err(FrameError::Crc);
            }
            if magic == MAGIC_LEGACY {
                return Ok(vec![(data, Some(Compression::Legacy))]);
            }
            let mut out = Vec::new();
            let mut i = 0;
            while i + 3 <= data.len() {
                let n =
                    (data[i] as usize) << 16 | (data[i + 1] as usize) << 8 | data[i + 2] as usize;
                i += 3;
                let end = (i + n).min(data.len());
                out.push((data[i..end].to_vec(), Some(Compression::Block)));
                i += n;
            }
            return Ok(out);
        }
    }
    Ok(vec![(payload.to_vec(), None)])
}

/// Decompresses an LZ4 block of known decompressed length.
pub fn lz4_block(src: &[u8], raw_len: usize) -> Result<Vec<u8>, FrameError> {
    let eof = FrameError::Lz4("data ended");
    let mut out: Vec<u8> = Vec::with_capacity(raw_len);
    let mut i = 0;
    let byte = |i: &mut usize| -> Result<usize, FrameError> {
        let b = *src.get(*i).ok_or(FrameError::Lz4("data ended"))?;
        *i += 1;
        Ok(b as usize)
    };
    while i < src.len() {
        let token = byte(&mut i)?;
        let mut lit = token >> 4;
        if lit == 15 {
            loop {
                let b = byte(&mut i)?;
                lit += b;
                if b != 255 {
                    break;
                }
            }
        }
        let lits = src.get(i..i + lit).ok_or(eof.clone())?;
        out.extend_from_slice(lits);
        i += lit;
        if out.len() > raw_len {
            break;
        }
        if i >= src.len() {
            break;
        }
        let off = byte(&mut i)? | byte(&mut i)? << 8;
        if off == 0 || off > out.len() {
            return Err(FrameError::Lz4("invalid offset"));
        }
        let mut ml = token & 15;
        if ml == 15 {
            loop {
                let b = byte(&mut i)?;
                ml += b;
                if b != 255 {
                    break;
                }
            }
        }
        ml += 4;
        if out.len() + ml > raw_len {
            return Err(FrameError::Lz4Length {
                got: out.len() + ml,
                want: raw_len,
            });
        }
        let start = out.len() - off;
        for k in 0..ml {
            let b = out[start + k];
            out.push(b);
        }
    }
    if out.len() != raw_len {
        return Err(FrameError::Lz4Length {
            got: out.len(),
            want: raw_len,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let mut f = Framer::new(false);
        let mut stream = frame(b"\x00\x01abc").unwrap();
        stream.extend(frame(b"\x00\x02").unwrap());
        assert_eq!(f.feed(&stream[..4]).unwrap(), Vec::<Vec<u8>>::new());
        assert_eq!(
            f.feed(&stream[4..]).unwrap(),
            vec![b"\x00\x01abc".to_vec(), b"\x00\x02".to_vec()]
        );
        assert_eq!(f.pending(), 0);
    }

    #[test]
    fn version_request_has_a_4_byte_length() {
        let mut f = Framer::new(true);
        let mut stream = vec![0, 0, 0, 3, 0x02, 0x9a, 7];
        stream.extend(frame(b"\x00\x05").unwrap());
        assert_eq!(
            f.feed(&stream).unwrap(),
            vec![vec![0x02, 0x9a, 7], vec![0, 5]]
        );
    }

    #[test]
    fn lz4_literals_and_match() {
        // "abcabcabc": 3 literals, then a match of 6 at offset 3
        let src = [0x32, b'a', b'b', b'c', 3, 0];
        assert_eq!(lz4_block(&src, 9).unwrap(), b"abcabcabc");
    }

    #[test]
    fn compressed_block() {
        let inner = [0u8, 0, 2, 0, 9];
        let lz = [(inner.len() as u8) << 4, 0, 0, 2, 0, 9];
        let mut p = vec![0xFF, 0xFF];
        p.extend(MAGIC_BLOCK.to_be_bytes());
        p.extend((inner.len() as u32).to_be_bytes());
        p.extend(crc32fast::hash(&inner).to_be_bytes());
        p.extend(lz);
        assert_eq!(
            downstream(&p).unwrap(),
            vec![(vec![0, 9], Some(Compression::Block))]
        );
    }
}
