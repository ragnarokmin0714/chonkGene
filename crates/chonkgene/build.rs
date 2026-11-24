fn main() {
    // The commit it is built from, on the log's first line: a log sent in
    // tells which build wrote it (Roger, 2026-11-24, sent a log from a
    // build before a fix, and nothing in it said so).
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=CHONKGENE_COMMIT={commit}");
    let git = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.git");
    println!("cargo:rerun-if-changed={git}/HEAD");
    println!("cargo:rerun-if-changed={git}/packed-refs");
    if let Ok(head) = std::fs::read_to_string(format!("{git}/HEAD"))
        && let Some(branch) = head.trim().strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed={git}/{branch}");
    }

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
