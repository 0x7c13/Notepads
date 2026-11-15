fn main() {
    println!("cargo::rustc-check-cfg=cfg(build_os_windows)");
    if std::env::consts::OS.contains("windows") {
        println!("cargo:rustc-cfg=build_os_windows");
    }
}
