use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use corvo_domain::image_mime;
use gpui_kit::{ClipboardEntry, ClipboardItem, Image, ImageFormat, ImageSource, SharedString};

// Agents turn larger images away.
const LIMIT: usize = 10 * 1024 * 1024;

// An image the dev put in the message, before it is sent: pasted, dropped or picked.
#[derive(Clone)]
pub struct Attached {
    pub id: u64,
    pub name: SharedString,
    pub ext: &'static str,
    pub bytes: Arc<Vec<u8>>,
    pub preview: ImageSource,
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// The images in what was pasted, and why any was turned away. None when the paste holds no
// image at all, so that it goes in as text.
pub fn pasted(item: &ClipboardItem) -> Option<(Vec<Attached>, Vec<String>)> {
    let mut images = Vec::new();
    let mut refused = Vec::new();
    let mut any = false;
    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(image) => {
                any = true;
                match from_image(image) {
                    Ok(image) => images.push(image),
                    Err(why) => refused.push(why),
                }
            }
            ClipboardEntry::ExternalPaths(paths) => {
                for path in paths.paths() {
                    if image_mime(path).is_none() {
                        continue;
                    }
                    any = true;
                    match from_path(path) {
                        Ok(image) => images.push(image),
                        Err(why) => refused.push(why),
                    }
                }
            }
            ClipboardEntry::String(_) => {}
        }
    }
    any.then_some((images, refused))
}

fn from_image(image: &Image) -> Result<Attached, String> {
    let ext = match image.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        other => {
            return Err(format!(
                "A pasted {} image is not one the agents read (PNG, JPEG, GIF, WebP)",
                other.extension().to_uppercase()
            ));
        }
    };
    if image.bytes.len() > LIMIT {
        return Err("The pasted image is over 10 MB".to_owned());
    }
    let id = next_id();
    Ok(Attached {
        id,
        name: format!("Pasted image {id}").into(),
        ext,
        bytes: Arc::new(image.bytes.clone()),
        preview: Arc::new(Image::from_bytes(image.format, image.bytes.clone())).into(),
    })
}

// An image from disk, read now: what is sent is what was attached.
pub fn from_path(path: &Path) -> Result<Attached, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let ext = match image_mime(path) {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        _ => {
            return Err(format!(
                "{name} is not an image the agents read (PNG, JPEG, GIF, WebP)"
            ));
        }
    };
    let size = std::fs::metadata(path)
        .map_err(|err| format!("{name}: {err}"))?
        .len();
    if size > LIMIT as u64 {
        return Err(format!("{name} is over 10 MB"));
    }
    let bytes = std::fs::read(path).map_err(|err| format!("{name}: {err}"))?;
    Ok(Attached {
        id: next_id(),
        name: name.into(),
        ext,
        bytes: Arc::new(bytes),
        preview: path.to_path_buf().into(),
    })
}
