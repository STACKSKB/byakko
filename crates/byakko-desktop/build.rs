fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icons/byakko.res");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let resource = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packaging/icons/byakko.res");
        println!(
            "cargo:rustc-link-arg-bin=byakko-desktop={}",
            resource.display()
        );
    }
}
