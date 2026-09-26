//! Decode covers on one worker; only pixel upload and model updates run on the UI.
//! Metadata invalidates the cache when a cover is replaced at the same path.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Instant, SystemTime};

use image::ImageEncoder;
use sha2::{Digest, Sha256};
use slint::{ComponentHandle, Image, Model, Rgba8Pixel, SharedPixelBuffer};

use crate::{AppWindow, log_info};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    modified: SystemTime,
}

impl Stamp {
    fn read(path: &Path) -> Option<Self> {
        let meta = path.metadata().ok()?;
        Some(Self {
            bytes: meta.len(),
            modified: meta.modified().ok()?,
        })
    }
}

struct Entry {
    stamp: Stamp,
    image: Image,
}

struct Job {
    path: PathBuf,
    stamp: Stamp,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<PathBuf, Entry>,
    sender: Option<mpsc::Sender<Job>>,
    window: Option<slint::Weak<AppWindow>>,
    done: i32,
    total: i32,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::default();
}

pub fn init(app: &AppWindow, cache_dir: PathBuf) {
    let (sender, jobs) = mpsc::channel::<Job>();
    let weak = app.as_weak();
    CACHE.with_borrow_mut(|cache| {
        cache.sender = Some(sender);
        cache.window = Some(weak.clone());
    });
    std::thread::spawn(move || {
        for job in jobs {
            let started = Instant::now();
            let pixels = decode(&job.path, &cache_dir);
            log_info!(
                "artwork",
                "carátula preparada en {} ms",
                started.elapsed().as_millis()
            );
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else { return };
                let image = CACHE.with_borrow_mut(|cache| {
                    cache.done += 1;
                    app.set_artwork_done(cache.done);
                    let entry = cache.entries.get_mut(&job.path)?;
                    if entry.stamp != job.stamp {
                        return None;
                    }
                    entry.image = pixels.map(Image::from_rgba8).unwrap_or_default();
                    Some(entry.image.clone())
                });
                let Some(image) = image else { return };
                let path = job.path.to_string_lossy();
                // Update just this image: keep selection, filters and metadata intact.
                for model in [app.get_games(), app.get_recent()] {
                    for index in 0..model.row_count() {
                        if let Some(mut row) = model.row_data(index)
                            && row.cover_path.as_str() == path
                        {
                            row.cover = image.clone();
                            model.set_row_data(index, row);
                        }
                    }
                }
                let mut hero = app.get_continue_game();
                if hero.cover_path.as_str() == path {
                    hero.cover = image.clone();
                    app.set_continue_game(hero);
                }
                let mut details = app.get_details_game();
                if details.cover_path.as_str() == path {
                    details.cover = image;
                    app.set_details_game(details);
                }
            });
        }
    });
}

/// Return immediately, using a placeholder until the worker delivers the cover.
pub fn load(path: &Path) -> Image {
    let Some(stamp) = Stamp::read(path) else {
        return Image::default();
    };
    CACHE.with_borrow_mut(|cache| {
        if let Some(entry) = cache.entries.get(path)
            && entry.stamp == stamp
        {
            return entry.image.clone();
        }
        let Some(sender) = &cache.sender else {
            return Image::default();
        };
        if sender
            .send(Job {
                path: path.into(),
                stamp,
            })
            .is_err()
        {
            return Image::default();
        }
        cache.entries.insert(
            path.into(),
            Entry {
                stamp,
                image: Image::default(),
            },
        );
        if cache.done == cache.total {
            cache.done = 0;
            cache.total = 0;
        }
        cache.total += 1;
        if let Some(app) = cache.window.as_ref().and_then(|w| w.upgrade()) {
            app.set_artwork_done(cache.done);
            app.set_artwork_total(cache.total);
        }
        Image::default()
    })
}

pub(crate) fn decode(path: &Path, cache_dir: &Path) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let stamp = Stamp::read(path)?;
    let mut hash = Sha256::new();
    hash.update(b"play-cover-1024-v1");
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update(stamp.bytes.to_le_bytes());
    hash.update(
        stamp
            .modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_nanos()
            .to_le_bytes(),
    );
    let cached = cache_dir.join(format!("{:x}.png", hash.finalize()));
    if let Ok(image) = image::open(&cached) {
        let rgba = image.into_rgba8();
        return Some(SharedPixelBuffer::clone_from_slice(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
        ));
    }
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    // Enough for the details view at 2x scale, without retaining giant source images.
    let rgba = if image.width() > 1024 || image.height() > 1024 {
        image.thumbnail(1024, 1024).into_rgba8()
    } else {
        image.into_rgba8()
    };
    // A missing/unwritable cache never prevents showing the original. Write only
    // generated thumbnails, atomically, so closing Play cannot leave half a PNG.
    if Stamp::read(path) == Some(stamp)
        && std::fs::create_dir_all(cache_dir).is_ok()
        && let Ok(mut temp) = tempfile::NamedTempFile::new_in(cache_dir)
    {
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            temp.as_file_mut(),
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Sub,
        );
        if encoder
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .is_ok()
        {
            let _ = temp.persist(&cached);
        }
    }
    Some(SharedPixelBuffer::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_preserve_aspect_and_alpha_at_bounded_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.png");
        image::RgbaImage::from_pixel(1200, 600, image::Rgba([50, 100, 200, 128]))
            .save(&path)
            .unwrap();
        let cache = dir.path().join("cache");
        let pixels = decode(&path, &cache).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (1024, 512));
        assert_eq!(pixels.as_slice()[0].a, 128);
        let cached = decode(&path, &cache).unwrap();
        assert_eq!(pixels.as_bytes(), cached.as_bytes());
        image::RgbaImage::from_pixel(10, 10, image::Rgba([200, 0, 0, 255]))
            .save(&path)
            .unwrap();
        assert_eq!(decode(&path, &cache).unwrap().width(), 10);
    }

    #[test]
    fn broken_cover_does_not_prevent_loading_other_games() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"not an image").unwrap();
        let cache = tempfile::tempdir().unwrap();
        assert!(decode(file.path(), cache.path()).is_none());
        let before = Stamp::read(file.path()).unwrap();
        std::fs::write(file.path(), b"replacement with a different size").unwrap();
        assert!(Stamp::read(file.path()) != Some(before));
    }
}
