fn main() {
    // GPUI's native layout/paint traversal with the embedded editor exceeds
    // MSVC's default 1 MiB main-thread reserve in debug builds. Reserve 8 MiB
    // for the GUI executable; Windows commits pages only as they are touched.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=studyapp=/STACK:8388608");
    }
}
