use std::io::Cursor;

use antex_core::Content;
use antex_runtime::ImageAttachment;
use image::DynamicImage;
use image::GenericImageView;
use image::ImageDecoder;
use image::ImageEncoder;
use image::ImageFormat;
use image::ImageReader;
use image::Rgba;
use image::RgbaImage;
use pretty_assertions::assert_eq;

#[test]
fn small_supported_images_keep_their_original_representation() {
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::Gif, "image/gif"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let pixels =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(64, 32, Rgba([10, 20, 30, 255])));
        let mut encoded = Cursor::new(Vec::new());
        pixels.write_to(&mut encoded, format).unwrap();
        let bytes = encoded.into_inner();
        let attachment = ImageAttachment::from_bytes(&bytes).unwrap();
        assert_eq!((attachment.width, attachment.height), (64, 32));
        assert_eq!(
            attachment.content,
            Content::Image {
                media_type: mime.into(),
                data: bytes.into()
            }
        );
    }
}

#[test]
fn large_images_preserve_aspect_ratio_and_alpha_within_output_budgets() {
    let pixels = RgbaImage::from_pixel(3000, 1500, Rgba([10, 20, 30, 128]));
    let attachment =
        ImageAttachment::from_rgba(pixels.width(), pixels.height(), pixels.into_raw()).unwrap();
    assert_eq!((attachment.width, attachment.height), (2048, 1024));
    let Content::Image { media_type, data } = attachment.content else {
        panic!("expected image")
    };
    assert_eq!(media_type, "image/png");
    assert!(data.len() <= antex_core::MAX_IMAGE_BYTES);
    let decoded = image::load_from_memory(&data).unwrap();
    assert_eq!(decoded.dimensions(), (2048, 1024));
    assert_eq!(decoded.get_pixel(1, 1).0[3], 128);
}

#[test]
fn resized_photos_apply_orientation_and_retain_rgb_color_profiles() {
    let exif = vec![
        0x49, 0x49, 0x2a, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let profile = b"0123456789abcdefRGB ";
    let pixels =
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(3000, 1000, Rgba([20, 40, 60, 255])));
    let mut encoded = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 90);
    encoder.set_exif_metadata(exif).unwrap();
    encoder.set_icc_profile(profile.to_vec()).unwrap();
    encoder.encode_image(&pixels).unwrap();
    let attachment = ImageAttachment::from_bytes(&encoded).unwrap();
    assert_eq!(attachment.height, 2048);
    assert!(attachment.width < attachment.height);
    let Content::Image { data, .. } = attachment.content else {
        panic!("expected image")
    };
    let mut decoder = ImageReader::new(Cursor::new(&data))
        .with_guessed_format()
        .unwrap()
        .into_decoder()
        .unwrap();
    assert_eq!(
        decoder.orientation().unwrap(),
        image::metadata::Orientation::NoTransforms
    );
    assert_eq!(decoder.icc_profile().unwrap(), Some(profile.to_vec()));
}

#[test]
fn invalid_pixels_and_dimension_overflow_are_rejected() {
    assert!(ImageAttachment::from_bytes(b"invalid image").is_err());
    assert!(ImageAttachment::from_rgba(u32::MAX, u32::MAX, Vec::new()).is_err());
    assert!(ImageAttachment::from_rgba(2, 2, vec![0; 15]).is_err());
}
