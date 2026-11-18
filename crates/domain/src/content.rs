use std::path::Path;

const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "svg", "bmp", "tif", "tiff", "ico", "pnm",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Image,
    Binary { size: u64 },
}

impl Content {
    pub fn is_image(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| IMAGES.contains(&ext.to_ascii_lowercase().as_str()))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        if bytes.contains(&0) {
            return Self::Binary { size };
        }
        match String::from_utf8(bytes) {
            Ok(text) => Self::Text(text),
            Err(_) => Self::Binary { size },
        }
    }
}
