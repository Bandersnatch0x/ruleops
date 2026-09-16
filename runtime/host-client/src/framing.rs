//! Length-delimited frame transport over byte streams (ADR-0030).
//!
//! Frame = 4-byte big-endian payload length + payload bytes. `max_frame_bytes`
//! bounds both directions; a longer frame is a hard protocol error, never a
//! truncated read.

use std::io::{Read, Write};

pub const DEFAULT_MAX_FRAME_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame of {size} bytes exceeds negotiated limit of {max} bytes")]
    Oversized { size: u64, max: u64 },
    #[error("stream ended mid-frame (expected {expected} more payload bytes)")]
    UnexpectedEof { expected: usize },
    #[error("stream ended cleanly at a frame boundary")]
    Eof,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct FrameWriter<W: Write> {
    inner: W,
    max_frame_bytes: u64,
}

impl<W: Write> FrameWriter<W> {
    pub fn new(inner: W, max_frame_bytes: u64) -> Self {
        Self {
            inner,
            max_frame_bytes,
        }
    }

    pub fn write_frame(&mut self, payload: &[u8]) -> Result<(), FrameError> {
        let len = payload.len() as u64;
        if len > self.max_frame_bytes {
            return Err(FrameError::Oversized {
                size: len,
                max: self.max_frame_bytes,
            });
        }
        self.inner.write_all(&(len as u32).to_be_bytes())?;
        self.inner.write_all(payload)?;
        self.inner.flush()?;
        Ok(())
    }
}

pub struct FrameReader<R: Read> {
    inner: R,
    max_frame_bytes: u64,
}

impl<R: Read> FrameReader<R> {
    pub fn new(inner: R, max_frame_bytes: u64) -> Self {
        Self {
            inner,
            max_frame_bytes,
        }
    }

    pub fn read_frame(&mut self) -> Result<Vec<u8>, FrameError> {
        let mut len_buf = [0u8; 4];
        if !read_exact_or_eof(&mut self.inner, &mut len_buf)? {
            return Err(FrameError::Eof);
        }
        let len = u32::from_be_bytes(len_buf) as u64;
        if len > self.max_frame_bytes {
            return Err(FrameError::Oversized {
                size: len,
                max: self.max_frame_bytes,
            });
        }
        let mut payload = vec![0u8; len as usize];
        self.inner.read_exact(&mut payload).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                FrameError::UnexpectedEof {
                    expected: len as usize,
                }
            } else {
                FrameError::Io(e)
            }
        })?;
        Ok(payload)
    }
}

/// Reads exactly `buf.len()` bytes; clean EOF at a boundary yields Ok(false).
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<bool, FrameError> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(false);
                }
                return Err(FrameError::UnexpectedEof {
                    expected: buf.len() - filled,
                });
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    Ok(true)
}
