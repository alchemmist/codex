use std::io;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;

use antex_core::Content;
use image::DynamicImage;
use image::ImageDecoder;
use image::ImageEncoder;
use image::ImageFormat;
use image::ImageReader;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;

const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_NORMALIZED_BYTES: usize = 3 * 1024 * 1024;
const MAX_DIMENSION: u32 = 2048;
const JPEG_QUALITY: u8 = 90;

pub struct ImageAttachment {
    pub content: Content,
    pub width: u32,
    pub height: u32,
}

impl ImageAttachment {
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("image must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take((MAX_SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(io::Error::other("image exceeds its input byte budget"));
        }
        let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        let format = reader
            .format()
            .ok_or_else(|| io::Error::other("unsupported image format"))?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let mut decoder = reader
            .into_decoder()
            .map_err(|_| io::Error::other("image decoder rejected the input"))?;
        let (width, height) = decoder.dimensions();
        if width <= MAX_DIMENSION && height <= MAX_DIMENSION && bytes.len() <= MAX_NORMALIZED_BYTES
        {
            DynamicImage::from_decoder(decoder)
                .map_err(|_| io::Error::other("invalid image pixels"))?;
            let media_type = match format {
                ImageFormat::Png => "image/png",
                ImageFormat::Jpeg => "image/jpeg",
                ImageFormat::Gif => "image/gif",
                ImageFormat::WebP => "image/webp",
                _ => return Err(io::Error::other("unsupported image format")),
            };
            return Ok(Self {
                content: Content::Image {
                    media_type: media_type.into(),
                    data: bytes.into(),
                },
                width,
                height,
            });
        }
        let orientation = decoder
            .orientation()
            .map_err(|_| io::Error::other("invalid image orientation metadata"))?;
        let profile =
            decoder.icc_profile().ok().flatten().filter(|profile| {
                profile.len() <= 256 * 1024 && profile.get(16..20) == Some(b"RGB ")
            });
        let mut image = DynamicImage::from_decoder(decoder)
            .map_err(|_| io::Error::other("image cannot be decoded within its resource budget"))?;
        image.apply_orientation(orientation);
        Self::normalize(image, format, profile)
    }

    pub fn from_rgba(width: u32, height: u32, bytes: Vec<u8>) -> io::Result<Self> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(io::Error::other("invalid clipboard image dimensions"));
        }
        let expected = u64::from(width) * u64::from(height) * 4;
        if expected > 128 * 1024 * 1024 || expected != bytes.len() as u64 {
            return Err(io::Error::other("invalid clipboard image byte budget"));
        }
        let buffer = image::RgbaImage::from_raw(width, height, bytes)
            .ok_or_else(|| io::Error::other("invalid clipboard image"))?;
        Self::normalize(
            DynamicImage::ImageRgba8(buffer),
            ImageFormat::Png,
            /*profile*/ None,
        )
    }

    fn normalize(
        mut image: DynamicImage,
        format: ImageFormat,
        profile: Option<Vec<u8>>,
    ) -> io::Result<Self> {
        if image.width() > MAX_DIMENSION || image.height() > MAX_DIMENSION {
            image = image.resize(MAX_DIMENSION, MAX_DIMENSION, FilterType::Triangle);
        }
        loop {
            let mut bytes = Vec::new();
            let media_type = if format == ImageFormat::Jpeg {
                let mut encoder = JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY);
                if let Some(profile) = &profile {
                    encoder
                        .set_icc_profile(profile.clone())
                        .map_err(io::Error::other)?;
                }
                encoder.encode_image(&image).map_err(io::Error::other)?;
                "image/jpeg"
            } else {
                let mut encoder = PngEncoder::new(&mut bytes);
                if let Some(profile) = &profile {
                    encoder
                        .set_icc_profile(profile.clone())
                        .map_err(io::Error::other)?;
                }
                image
                    .write_with_encoder(encoder)
                    .map_err(io::Error::other)?;
                "image/png"
            };
            if bytes.len() <= MAX_NORMALIZED_BYTES {
                return Ok(Self {
                    content: Content::Image {
                        media_type: media_type.into(),
                        data: bytes.into(),
                    },
                    width: image.width(),
                    height: image.height(),
                });
            }
            let width = (image.width() * 3 / 4).max(1);
            let height = (image.height() * 3 / 4).max(1);
            if width == image.width() && height == image.height() {
                return Err(io::Error::other("image cannot fit the output byte budget"));
            }
            image = image.resize(width, height, FilterType::Triangle);
        }
    }
}
