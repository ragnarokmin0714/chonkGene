fn main() {
    // Embed the font file as-is; the software renderer draws glyphs at run time.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/settings.slint", config)
        .expect("the settings UI compiles");
}
