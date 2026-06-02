use std::io::{self, BufReader, Read};
use std::sync::mpsc;
use std::thread;

// ─────────────────────────────────────────────────────────────────────────────
// MXFR wire format (little-endian throughout)
//
//  Offset  Size          Type     Field
//  ──────  ─────         ───────  ─────────────────────────────────────────────────
//   0       4            u8[4]    magic = b"MXFR"
//   4       4            u32 LE   width  W
//   8       4            u32 LE   height H
//  12       4            u32 LE   channels C  (1 = scalar, 3 = RGB)
//  16       8            f64 LE   timestamp  (caller's physical time, any unit)
//  24     W*H*C*4        f32 LE[]  row-major pixel data (C values per pixel)
// ─────────────────────────────────────────────────────────────────────────────

pub struct MxfrFrame {
    pub width:     u32,
    pub height:    u32,
    pub channels:  u32,
    pub timestamp: f64,
    pub data:      Vec<f32>,
}

/// Spawn a background thread that reads MXFR frames from stdin and sends
/// them over the returned channel.  The channel is closed when stdin reaches
/// EOF or an unrecoverable read error occurs.
pub fn spawn_reader() -> mpsc::Receiver<MxfrFrame> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let stdin  = io::stdin();
        // 4 MiB buffer: large frames (e.g. 1000×1000 = 4 MB) otherwise take
        // hundreds of tiny 8 KiB reads each, starving the render thread.
        let mut rd = BufReader::with_capacity(4 << 20, stdin.lock());
        loop {
            match read_frame(&mut rd) {
                Ok(frame) => {
                    if tx.send(frame).is_err() {
                        break; // receiver dropped — window was closed
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    break; // clean EOF — solver finished
                }
                Err(e) => {
                    eprintln!("mxfr reader: {e}");
                    break;
                }
            }
        }
    });
    rx
}

fn read_frame(r: &mut impl Read) -> io::Result<MxfrFrame> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != b"MXFR" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bad magic: {:?}", magic),
        ));
    }

    let width     = read_u32_le(r)?;
    let height    = read_u32_le(r)?;
    let channels  = read_u32_le(r)?;
    let timestamp = read_f64_le(r)?;

    let n = (width * height * channels) as usize;
    let mut bytes = vec![0u8; n * 4];
    r.read_exact(&mut bytes)?;

    let data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();

    Ok(MxfrFrame { width, height, channels, timestamp, data })
}

fn read_u32_le(r: &mut impl Read) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_f64_le(r: &mut impl Read) -> io::Result<f64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(f64::from_le_bytes(buf))
}
