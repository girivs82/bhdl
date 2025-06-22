# Release Checklist

This checklist should be followed when preparing a new release of BHDL.

## Pre-Release

### Code Quality
- [ ] All tests pass: `cargo test --all`
- [ ] No clippy warnings: `cargo clippy --all -- -D warnings`
- [ ] Code is formatted: `cargo fmt --all -- --check`
- [ ] Documentation builds: `cargo doc --no-deps --all-features`
- [ ] Examples run successfully
- [ ] No security advisories: `cargo audit`

### Version Updates
- [ ] Update version in all `Cargo.toml` files
- [ ] Update version in README.md badges
- [ ] Update CHANGELOG.md with release notes
- [ ] Update defensive publication dates if applicable

### Documentation
- [ ] API documentation is complete
- [ ] README.md is up to date
- [ ] Examples work with new version
- [ ] Migration guide for breaking changes

### Testing
- [ ] Test on Linux (Ubuntu latest LTS)
- [ ] Test on macOS (latest stable)
- [ ] Test on Windows (latest stable)
- [ ] Component database integration works
- [ ] Visualizer generates valid SVGs

## Release Process

### 1. Create Release Branch
```bash
git checkout -b release/v0.1.0
```

### 2. Final Checks
- [ ] Run full test suite
- [ ] Build release binaries: `cargo build --release`
- [ ] Test CLI commands with release build
- [ ] Verify all examples parse correctly

### 3. Tag Release
```bash
git tag -a v0.1.0 -m "Release version 0.1.0"
git push origin v0.1.0
```

### 4. GitHub Release
- [ ] Create release from tag
- [ ] Add release notes from CHANGELOG.md
- [ ] Upload pre-built binaries:
  - [ ] Linux x86_64
  - [ ] Linux ARM64
  - [ ] macOS x86_64
  - [ ] macOS ARM64 (Apple Silicon)
  - [ ] Windows x86_64
- [ ] Include SHA256 checksums

### 5. Publish to crates.io (if applicable)
```bash
# Publish in dependency order
cargo publish -p bhdl-common
cargo publish -p bhdl-parser
cargo publish -p bhdl-ast
# ... etc
```

### 6. Post-Release
- [ ] Update main branch version to next development version
- [ ] Add "Unreleased" section to CHANGELOG.md
- [ ] Announce release:
  - [ ] GitHub Discussions
  - [ ] Twitter/Social Media
  - [ ] Reddit (r/rust, r/electronics)
  - [ ] Hacker News (if major release)
- [ ] Update documentation site
- [ ] Archive defensive publications with release date

## Versioning Policy

BHDL follows [Semantic Versioning](https://semver.org/):

- **MAJOR**: Incompatible language changes
- **MINOR**: New features, backward compatible
- **PATCH**: Bug fixes, backward compatible

### Pre-1.0 Policy
- Breaking changes allowed in MINOR versions
- PATCH versions for bug fixes only
- Clear migration guides for all breaking changes

### Post-1.0 Policy
- Language specification frozen
- Breaking changes require MAJOR version
- Deprecation period of at least one MINOR version

## Binary Distribution

### Supported Platforms
- Linux: x86_64-unknown-linux-gnu (glibc 2.17+)
- Linux: x86_64-unknown-linux-musl (static)
- macOS: x86_64-apple-darwin (10.12+)
- macOS: aarch64-apple-darwin (11.0+)
- Windows: x86_64-pc-windows-msvc (Windows 10+)

### Binary Naming Convention
```
bhdl-cli-<version>-<target>.<ext>
```

Examples:
- `bhdl-cli-0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `bhdl-cli-0.1.0-x86_64-apple-darwin.tar.gz`
- `bhdl-cli-0.1.0-x86_64-pc-windows-msvc.zip`

## Emergency Release Process

For critical security fixes:

1. Create patch on security branch
2. Test minimal fix thoroughly
3. Release as PATCH version
4. Disclose vulnerability after release
5. Credit reporter in SECURITY.md