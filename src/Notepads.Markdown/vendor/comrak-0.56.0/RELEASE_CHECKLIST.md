* Bump version in `Cargo.toml`, ensuring you respect semver --- if you added a
  new member to a struct, that's a breaking change!
* Run <https://github.com/kivikakk/comrak/actions/workflows/release.yml>.
* Check out the created PR, write proper notes in `CHANGELOG.md`, commit and
  push.
* Merge the PR.
* `git tag vX.Y.Z`, push the tag, `cargo publish`.
* `script/build-releases` to build the four \*nix releases (needs `nix-darwin`
  with a Linux builder), and make the two Windows ones manually.
* Attach the releases to a new GitHub release, sourcing the text for it from
  the changelog.
