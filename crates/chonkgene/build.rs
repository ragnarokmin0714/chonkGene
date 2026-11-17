fn main() {
    // Embed the font file as-is; the software renderer draws glyphs at run time.
    // Its words in Japanese and English come from i18n/<lang>/LC_MESSAGES/
    // chonkgene.po, keyed by the Chinese (`@tr`), as the Rust side's
    // `chonkgene_core::i18n::tr` reads them.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
        .with_bundled_translations("../../i18n")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    println!("cargo:rerun-if-changed=../../i18n");
    slint_build::compile_with_config("ui/settings.slint", config)
        .expect("the settings UI compiles");

    // The icon Explorer, the taskbar and Alt-Tab show, under resource id 1
    // (chonkgene_platform::use_exe_icon loads it back), plus the name Task
    // Manager shows. Baked by tools/iconbake.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let ico = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icon/chonkgene.ico"
        );
        println!("cargo:rerun-if-changed={ico}");
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico)
            .set("ProductName", "chonkGene")
            .set("FileDescription", "肉肉賴桌");
        res.compile().expect("the Windows resources compile");
    }
}
