pub(crate) fn make_test_tui() -> std::io::Result<super::Tui<crate::test_backend::VT100Backend>> {
    super::Tui::new(crate::test_backend::VT100Backend::new(80, 24))
}
