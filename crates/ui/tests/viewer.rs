use corvo_ui::viewer::size;

#[test]
fn sizes_read_like_a_file_manager() {
    assert_eq!(size(512), "512 B");
    assert_eq!(size(2048), "2.0 KB");
    assert_eq!(size(2_411_725), "2.3 MB");
    assert_eq!(size(5 * 1024 * 1024 * 1024), "5.0 GB");
}
