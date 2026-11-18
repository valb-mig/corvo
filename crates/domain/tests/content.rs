use std::path::Path;

use corvo_domain::Content;

#[test]
fn detects_images_by_extension() {
    assert!(Content::is_image(Path::new("/p/a.png")));
    assert!(Content::is_image(Path::new("/p/a.SVG")));
    assert!(!Content::is_image(Path::new("/p/a.rs")));
    assert!(!Content::is_image(Path::new("/p/png")));
}

#[test]
fn utf8_without_nul_is_text() {
    let content = Content::from_bytes("olá".as_bytes().to_vec());

    assert_eq!(content, Content::Text("olá".into()));
}

#[test]
fn nul_or_invalid_utf8_is_binary() {
    assert_eq!(
        Content::from_bytes(vec![b'a', 0, b'b']),
        Content::Binary { size: 3 }
    );
    assert_eq!(
        Content::from_bytes(vec![0xff, 0xfe]),
        Content::Binary { size: 2 }
    );
}
