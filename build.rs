fn main() {
    println!("cargo:rerun-if-changed=assets/guiyuX.rc");
    println!("cargo:rerun-if-changed=assets/guiyuX.ico");
    embed_resource::compile_for_everything(
        "assets/guiyuX.rc",
        embed_resource::ParamsIncludeDirs(["assets"]),
    )
    .manifest_required()
    .expect("failed to embed the GuiyuX application icon");
}
