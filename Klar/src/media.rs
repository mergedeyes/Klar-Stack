/// Image processing — resize, generate variants, strip EXIF.
///
/// EXIF stripping happens automatically: when we decode the image and
/// re-encode it as WebP, all metadata (GPS, device info, timestamps)
/// is dropped. The image crate doesn't copy EXIF on re-encode.
/// This is a core privacy feature of Klar.
///
/// One piece of EXIF *is* read before it's discarded, though: the
/// orientation tag. Phone cameras commonly store photos using the
/// sensor's native (often landscape) orientation and rely on this tag
/// to say "rotate/flip this for display" — image decoders don't apply
/// that automatically, so without reading it first, a portrait photo
/// silently comes out sideways once EXIF is stripped on re-encode.

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;

/// The three variants we generate for every uploaded image
pub struct ProcessedImage {
    /// 150x150 square center-crop (profile grids)
    pub thumb: Vec<u8>,
    /// 640px wide, maintain aspect ratio (mobile feed)
    pub medium: Vec<u8>,
    /// 1080px wide, maintain aspect ratio (full view)
    pub full: Vec<u8>,
    /// Original dimensions
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct ProcessingError(pub String);

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image processing error: {}", self.0)
    }
}

/// Process an uploaded image: validate, resize, and generate all variants.
/// Returns the processed variants as byte vectors ready to be saved.
pub fn process_image(raw_bytes: &[u8]) -> Result<ProcessedImage, ProcessingError> {
    // Decode via the two-step decoder API (rather than the one-shot
    // ImageReader::decode() convenience method) specifically so we can
    // read the EXIF orientation tag before it's gone.
    let decoder = ImageReader::new(Cursor::new(raw_bytes))
        .with_guessed_format()
        .map_err(|e| ProcessingError(format!("Failed to read image: {}", e)))?
        .into_decoder()
        .map_err(|e| ProcessingError(format!("Failed to decode image: {}", e)))?;

    // Formats without EXIF support (or images with no orientation tag at
    // all) fall back to NoTransforms — i.e. use the pixels exactly as
    // decoded, which is the correct behavior for those cases anyway.
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);

    let mut img = DynamicImage::from_decoder(decoder)
        .map_err(|e| ProcessingError(format!("Failed to decode image: {}", e)))?;

    // Rearranges the actual pixel data to match how the photo was meant
    // to be viewed. From here on the image is "correctly" oriented, so
    // every downstream step (crop, resize, encode) just works on pixels
    // with no further EXIF-awareness needed.
    img.apply_orientation(orientation);

    let width = img.width();
    let height = img.height();

    // Reject tiny images
    if width < 100 || height < 100 {
        return Err(ProcessingError("Image must be at least 100x100 pixels".to_string()));
    }

    // Generate variants
    let thumb = generate_thumbnail(&img)?;
    let medium = resize_to_width(&img, 640)?;
    let full = resize_to_width(&img, 1080)?;

    Ok(ProcessedImage {
        thumb,
        medium,
        full,
        width,
        height,
    })
}

/// Generate a 150x150 square center-crop thumbnail
fn generate_thumbnail(img: &DynamicImage) -> Result<Vec<u8>, ProcessingError> {
    // crop_imm takes (x, y, width, height) — we center the crop
    let size = img.width().min(img.height());
    let x = (img.width() - size) / 2;
    let y = (img.height() - size) / 2;

    let cropped = img.crop_imm(x, y, size, size);
    let resized = cropped.resize_exact(150, 150, image::imageops::FilterType::Lanczos3);

    encode_webp(&resized)
}

/// Resize to a target width, maintaining aspect ratio.
/// If the image is smaller than the target, don't upscale — return as-is.
fn resize_to_width(img: &DynamicImage, target_width: u32) -> Result<Vec<u8>, ProcessingError> {
    if img.width() <= target_width {
        // Don't upscale — just re-encode (which strips EXIF)
        return encode_webp(img);
    }

    let ratio = target_width as f64 / img.width() as f64;
    let target_height = (img.height() as f64 * ratio) as u32;

    let resized = img.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3);
    encode_webp(&resized)
}

/// Encode a DynamicImage as WebP with good quality.
/// This step is what strips EXIF — re-encoding creates a clean image
/// with no metadata from the original.
fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>, ProcessingError> {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);

    img.write_to(&mut cursor, ImageFormat::WebP)
        .map_err(|e| ProcessingError(format!("Failed to encode WebP: {}", e)))?;

    Ok(buf)
}
