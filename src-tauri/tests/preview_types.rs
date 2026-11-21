use viewera_lib::files::preview_type_for_extension;

#[test]
fn selects_renderers_case_insensitively() {
    assert_eq!(preview_type_for_extension("PDF"), "pdf");
    assert_eq!(preview_type_for_extension("WebP"), "image");
    assert_eq!(preview_type_for_extension("XLSX"), "spreadsheet");
}

#[test]
fn does_not_treat_legacy_binary_word_or_powerpoint_as_ooxml() {
    assert_eq!(preview_type_for_extension("doc"), "unsupported");
    assert_eq!(preview_type_for_extension("ppt"), "unsupported");
}

#[test]
fn recognizes_common_source_and_config_text_formats() {
    assert_eq!(preview_type_for_extension("sh"), "text");
    assert_eq!(preview_type_for_extension("SQL"), "text");
    assert_eq!(preview_type_for_extension("env"), "text");
    assert_eq!(preview_type_for_extension("JSONC"), "json");
}

#[test]
fn recognizes_audio_video_and_zip_preview_types() {
    assert_eq!(preview_type_for_extension("MP3"), "audio");
    assert_eq!(preview_type_for_extension("webm"), "video");
    assert_eq!(preview_type_for_extension("ZIP"), "zip");
}

#[test]
fn recognizes_html_and_mhtml_as_renderable_documents() {
    assert_eq!(preview_type_for_extension("HTML"), "html");
    assert_eq!(preview_type_for_extension("htm"), "html");
    assert_eq!(preview_type_for_extension("MHTML"), "mhtml");
    assert_eq!(preview_type_for_extension("mht"), "mhtml");
}
