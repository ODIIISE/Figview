//! ZIP archive handling for `.fig` files (store mode).

use crate::error::ParseError;
use std::collections::HashMap;
use std::io::Read;

pub struct FigArchive {
    pub canvas_fig: Vec<u8>,
    pub meta_json: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub images: HashMap<String, Vec<u8>>,
}

pub fn open_archive(path: &str) -> Result<FigArchive, ParseError> {
    read_archive(std::io::BufReader::new(std::fs::File::open(path)?))
}

/// Magic prefix of a raw (non-ZIP) kiwi stream.
const KIWI_MAGIC: &[u8] = b"fig-kiwi";

pub fn read_archive<R: Read + std::io::Seek>(mut reader: R) -> Result<FigArchive, ParseError> {
    // Format sniffing: some distributed .fig files are raw `fig-kiwi`
    // streams (canvas data only — no meta.json, thumbnail or images),
    // not ZIP containers. Detect by magic bytes before assuming ZIP.
    let mut magic = [0u8; 8];
    let start = reader.stream_position()?;
    reader.read_exact(&mut magic)?;
    reader.seek(std::io::SeekFrom::Start(start))?;

    if &magic == KIWI_MAGIC {
        let mut canvas_fig = Vec::new();
        reader.read_to_end(&mut canvas_fig)?;
        return Ok(FigArchive {
            canvas_fig,
            meta_json: Vec::new(),
            thumbnail: Vec::new(),
            images: HashMap::new(),
        });
    }

    let mut archive = zip::ZipArchive::new(reader)?;
    let mut canvas_fig = None;
    let mut meta_json = Vec::new();
    let mut thumbnail = Vec::new();
    let mut images = HashMap::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        match name.as_str() {
            "canvas.fig" => {
                buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf)?;
                canvas_fig = Some(buf);
            }
            "meta.json" => {
                entry.read_to_end(&mut meta_json)?;
            }
            "thumbnail.png" => {
                entry.read_to_end(&mut thumbnail)?;
            }
            _ if name.starts_with("images/") => {
                if let Some(hash) = name.strip_prefix("images/") {
                    if !hash.is_empty() {
                        entry.read_to_end(&mut buf)?;
                        images.insert(hash.to_string(), buf);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(FigArchive {
        canvas_fig: canvas_fig.ok_or_else(|| ParseError::MissingEntry("canvas.fig".into()))?,
        meta_json,
        thumbnail,
        images,
    })
}
