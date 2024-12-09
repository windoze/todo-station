fn main() {
    let config = slint_build::CompilerConfiguration::default()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
        .with_style("cupertino".into());
    slint_build::compile_with_config("ui/app-window.slint", config).expect("Slint build failed");
}
