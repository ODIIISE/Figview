//! Binary parsing of `canvas.fig` — the inner format of `.fig` files.
//!
//! Layout:
//! ```text
//! [prelude: 8B][version: u32 LE][chunk0_len][chunk0]...[chunkN_len][chunkN]
//! ```
//! Chunk 0: Kiwi schema (deflateRaw). Chunk 1: message (deflateRaw or zstd).

use crate::error::ParseError;

pub struct DecompressedCanvasFig {
    pub prelude: String,
    pub version: u32,
    pub schema_bytes: Vec<u8>,
    pub message_bytes: Vec<u8>,
}

pub fn parse_canvas_fig(data: &[u8]) -> Result<DecompressedCanvasFig, ParseError> {
    if data.len() < 12 {
        return Err(ParseError::Other("canvas.fig too small (< 12 bytes)".into()));
    }

    let prelude = String::from_utf8_lossy(&data[0..8]).to_string();
    let version = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

    let mut pos: usize = 12;
    let mut chunks: Vec<Vec<u8>> = Vec::new();

    while pos + 4 <= data.len() {
        let chunk_len = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        pos += 4;
        if pos + chunk_len > data.len() { break; }
        chunks.push(data[pos..pos + chunk_len].to_vec());
        pos += chunk_len;
    }

    if chunks.len() < 2 {
        return Err(ParseError::Other(format!(
            "Expected at least 2 chunks, got {}", chunks.len()
        )));
    }

    Ok(DecompressedCanvasFig {
        prelude,
        version,
        schema_bytes: decompress_deflate_raw(&chunks[0])?,
        message_bytes: decompress_message(&chunks[1])?,
    })
}

fn decompress_deflate_raw(data: &[u8]) -> Result<Vec<u8>, ParseError> {
    use flate2::read::DeflateDecoder;
    use std::io::Read;
    let mut decoder = DeflateDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|e| ParseError::Deflate(e.to_string()))?;
    Ok(out)
}

fn decompress_message(data: &[u8]) -> Result<Vec<u8>, ParseError> {
    // Zstandard magic: 0x28 0xB5 0x2F 0xFD
    if data.len() >= 4 && &data[0..4] == &[0x28, 0xB5, 0x2F, 0xFD] {
        zstd::stream::decode_all(std::io::Cursor::new(data))
            .map_err(|e| ParseError::Zstd(e.to_string()))
    } else {
        decompress_deflate_raw(data)
    }
}