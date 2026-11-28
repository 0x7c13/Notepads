#[test]
fn public_api() {
    // Install a compatible nightly toolchain if it is missing
    rustup_toolchain::install(public_api::MINIMUM_NIGHTLY_RUST_VERSION).unwrap();

    // Build rustdoc JSON
    let rustdoc_json = rustdoc_json::Builder::default()
        .toolchain(public_api::MINIMUM_NIGHTLY_RUST_VERSION)
        .build()
        .unwrap();

    // Derive the public API from the rustdoc JSON
    let public_api = public_api::Builder::from_rustdoc_json(rustdoc_json)
        .omit_blanket_impls(true)
        .build()
        .unwrap();

    // Assert that the public API matches the latest snapshot.
    // Run with env var `UPDATE_SNAPSHOTS=yes` to update the snapshot.
    public_api.assert_eq_or_update("./tests/snapshots/public-api.txt");
}
