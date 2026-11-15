//! Private library utilities that are not exposed to clients since we don't
//! want to make semver guarantees about them

use std::path::Path;

use walkdir::WalkDir;

/// Private helper to walk a dir and also follow symbolic links.
pub fn walk_dir<P: AsRef<Path>>(folder: P) -> WalkDir {
    WalkDir::new(folder).follow_links(true)
}

#[cfg(all(test, feature = "parsing"))]
pub mod testdata {
    use std::sync::LazyLock;

    use crate::parsing::SyntaxSet;

    /// The [`SyntaxSet`] loaded from the `testdata/Packages` folder
    ///
    /// Shared here to avoid re-doing a particularly costly construction in various tests
    pub static PACKAGES_SYN_SET: LazyLock<SyntaxSet> =
        LazyLock::new(|| SyntaxSet::load_from_folder("testdata/Packages").unwrap());
}
