# KAIRO-FW Changelog

## [0.3.0-1] - 2026-05-09

### Added
- **Real KAIRO Management Tool:** Replaced the placeholder wrapper with the actual `kairo` binary built from source.
- **Production Stack Integration:** Bundled the real `kairo-core`, `kairo-net`, and `kairo-sec` components.
- **Local Rule definitions:** Included `kairo-sec` rulepacks (CVE and non-CVE) in the package for local verification.
- **Improved User Guides:** Updated all documentation to accurately describe the real commands and technical boundaries.

### Changed
- **Packaging:** Switched to architecture-specific packaging (amd64) to support the compiled binary.
- **Path Discovery:** Updated the stack to correctly find rule metadata at the installed path (`/usr/share/kairo-fw/rules`).

### Security
- **Verified Payload:** Maintained strict exclusion of the ethics engine and legacy components.
- **Release Verification:** Provided instructions for verifying the ELF binary and local rule integrity.

## [0.2.0] - 2026-05-04

### Added
- Initial baseline for the Debian package structure.
- Basic version tracking.
