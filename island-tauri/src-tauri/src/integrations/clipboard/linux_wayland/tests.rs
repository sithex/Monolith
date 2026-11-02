use super::*;
fn png(width: u32, height: u32) -> Result<Vec<u8>, image::ImageError> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 255]));
    let mut cursor = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(cursor.into_inner())
}
#[test]
fn png_decoder_requires_real_png_and_preserves_actual_dimensions() {
    assert!(decode_png(vec![1, 2, 3]).is_err());
    assert!(matches!(
        decode_png(png(2, 3).unwrap()).unwrap(),
        ClipPayload::Image {
            width: 2,
            height: 3,
            ..
        }
    ));
}
#[test]
fn png_decoder_enforces_independent_width_height_and_encoded_byte_caps() {
    assert!(decode_png(png(4097, 1).unwrap()).is_err());
    assert!(decode_png(png(1, 4097).unwrap()).is_err());
    assert!(decode_png(vec![0; PNG_CAP + 1]).is_err());
    assert!(decode_png(png(4096, 1).unwrap()).is_ok());
    assert!(decode_png(png(1, 4096).unwrap()).is_ok());
}
#[test]
fn uri_list_parses_comments_crlf_and_preserves_uri_encoding() {
    assert_eq!(
        parse_uris(b"# comment\r\nfile:///a%20b.txt\r\n\r\nfile:///c.txt\n").unwrap(),
        vec!["file:///a%20b.txt", "file:///c.txt"]
    );
    assert!(parse_uris(b"file:///a\0b").is_err());
    assert!(parse_uris(&[255]).is_err());
    assert!(parse_uris("file:///a\n".repeat(4097).as_bytes()).is_err());
}
#[test]
fn mime_preference_selects_file_list_then_png_then_utf8_text() {
    let mimes = vec![
        "text/plain".into(),
        "UTF8_STRING".into(),
        "image/png".into(),
        "text/uri-list".into(),
    ];
    assert!(matches!(preferred(&mimes), Some((_, ReadKind::Files))));
    assert!(matches!(preferred(&mimes[..3]), Some((_, ReadKind::Png))));
    assert_eq!(
        preferred(&mimes[..2]).map(|(m, _)| m),
        Some("UTF8_STRING".into())
    );
    assert!(preferred(&["application/octet-stream".into()]).is_none());
}
#[test]
fn copy_data_sources_preserve_text_png_and_file_list_bytes() {
    let text = send_data(&ClipPayload::Text("héllo".into()));
    assert_eq!(text.mimes[0].0, "text/plain;charset=utf-8");
    assert!(text
        .mimes
        .iter()
        .all(|(_, data)| data.as_ref() == "héllo".as_bytes()));
    let files = send_data(&ClipPayload::Files(vec![
        "file:///one".into(),
        "file:///two".into(),
    ]));
    assert_eq!(files.mimes[0].1.as_ref(), b"file:///one\r\nfile:///two\r\n");
    let data = png(2, 3).unwrap();
    let encoded = send_data(&ClipPayload::Image {
        png: data.clone(),
        width: 2,
        height: 3,
    });
    assert_eq!(encoded.mimes[0].1.as_ref(), data.as_slice());
}
