fn main() {
    let is_nightly = version_check::is_feature_flaggable() == Some(true);
    let is_at_least_1_58 = version_check::is_min_version("1.58.0").unwrap_or(false);
    let is_at_least_1_80 = version_check::is_min_version("1.80.0").unwrap_or(false);

    if !is_at_least_1_58 {
        println!("cargo:warning=slotmap requires rustc => 1.58.0");
    }

    if is_at_least_1_80 || is_nightly {
        println!("cargo:rustc-check-cfg=cfg(nightly)");
    }

    if is_nightly {
        println!("cargo:rustc-cfg=nightly");
    }
}
