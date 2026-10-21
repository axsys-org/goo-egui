//! Bounded asynchronous image decoding. URLs describe pixels, never authority.
use anyhow::{Result, ensure};
use base64::Engine;
use eframe::egui::*;
use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Cache(Arc<Mutex<HashMap<String, Image>>>);
enum Image {
    Loading,
    Pixels(ColorImage),
    Ready(TextureHandle),
    Failed(String),
}
pub fn set_server(ctx: &Context, server: &str) {
    ctx.data_mut(|d| {
        d.insert_temp(
            Id::new("goo-image-server"),
            server.trim_end_matches('/').to_owned(),
        )
    });
}
pub fn texture(ctx: &Context, source: &str) -> Result<Option<TextureHandle>, String> {
    let resolved;
    let source = if source.starts_with('/') {
        let base = ctx
            .data(|d| d.get_temp::<String>(Id::new("goo-image-server")))
            .ok_or_else(|| "Relative image has no connected server".to_owned())?;
        resolved = format!("{base}{source}");
        &resolved
    } else {
        source
    };
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Cache>(Id::new("goo-image-cache"))
            .clone()
    });
    let mut entries = cache.0.lock().unwrap();
    match entries.get_mut(source) {
        Some(Image::Ready(t)) => Ok(Some(t.clone())),
        Some(Image::Failed(e)) => Err(e.clone()),
        Some(Image::Loading) => Ok(None),
        Some(Image::Pixels(p)) => {
            let t = ctx.load_texture(source, p.clone(), TextureOptions::LINEAR);
            entries.insert(source.into(), Image::Ready(t.clone()));
            Ok(Some(t))
        }
        None => {
            if entries.len() >= 128 {
                return Err("Image cache capacity reached (128 sources)".into());
            }
            entries.insert(source.into(), Image::Loading);
            let source = source.to_owned();
            let ctx = ctx.clone();
            let cache = cache.clone();
            std::thread::spawn(move || {
                let result = decode(&source)
                    .map(Image::Pixels)
                    .unwrap_or_else(|e| Image::Failed(e.to_string()));
                cache.0.lock().unwrap().insert(source, result);
                ctx.request_repaint();
            });
            Ok(None)
        }
    }
}
pub fn decode(src: &str) -> Result<ColorImage> {
    let (bytes, svg) = if let Some(data) = src.strip_prefix("data:") {
        let (header, body) = data
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("Invalid data image"))?;
        let bytes = if header.ends_with(";base64") {
            base64::engine::general_purpose::STANDARD.decode(body)?
        } else {
            percent_encoding::percent_decode_str(body).collect()
        };
        (bytes, header.starts_with("image/svg+xml"))
    } else {
        ensure!(
            !src.starts_with('/'),
            "Relative image needs its owning server"
        );
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?
            .get(src)
            .send()?
            .error_for_status()?;
        let svg = response
            .headers()
            .get("content-type")
            .is_some_and(|v| v.to_str().unwrap_or("").contains("image/svg+xml"));
        let mut bytes = vec![];
        response.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        (bytes, svg)
    };
    ensure!(bytes.len() <= 8 * 1024 * 1024, "Image exceeds 8 MiB");
    if svg {
        let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default())?;
        let size = tree.size().to_int_size();
        ensure!(
            size.width() <= 4096
                && size.height() <= 4096
                && u64::from(size.width()) * u64::from(size.height()) <= 4_194_304,
            "SVG dimensions exceed limit"
        );
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or_else(|| anyhow::anyhow!("Empty SVG"))?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        // tiny-skia stores premultiplied sRGBA.
        let pixels = pixmap
            .pixels()
            .iter()
            .map(|p| Color32::from_rgba_premultiplied(p.red(), p.green(), p.blue(), p.alpha()))
            .collect();
        Ok(ColorImage::new(
            [size.width() as usize, size.height() as usize],
            pixels,
        ))
    } else {
        let mut reader =
            image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        let img = reader.decode()?.to_rgba8();
        ensure!(
            u64::from(img.width()) * u64::from(img.height()) <= 4_194_304,
            "Image dimensions exceed limit"
        );
        Ok(ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            &img,
        ))
    }
}
