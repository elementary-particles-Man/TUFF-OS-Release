# KAIRO-FW 0.3.0 Release Notes

Date: 2026-05-09
Version: 0.3.0

## What's New?
This version introduces the **Real KAIRO Management Tool** (`kairo`). Unlike previous releases, this is the actual binary built from the TUFF-KAIRO source, providing a genuine interface to the KAIRO core, networking, and security stack.

## Included in this version
- **Real Management Tool:** The actual `kairo` binary with full support for status, auditing, and rule verification.
- **Security Rule Stack:** Bundled `kairo-sec` rule definitions and metadata (CVE and non-CVE).
- **Core Integration:** Direct integration with the real `kairo-core` and `kairo-net` logic.
- **Improved Metadata:** Support for Dirty Frag / Copy Fail 2 alias tracking.

## Not included
- **Ethics Engine:** The ethics engine runtime is explicitly excluded from this package.
- **Automatic Firewall Changes:** This release does not automatically change your system's firewall or kernel settings.
- **Legacy Components:** Strictly excludes old `kairo-daemon` and `KAIRO-OLD` payloads.

## Known Limitations
- The tool performs comprehensive status and path verification but does not perform live security enforcement in this release.

## Verification
You can verify that your installation is functional by running:
```sh
kairo status
kairo security matrix status
```
Technical verification steps for the package file itself are available in the [Verification Guide](VERIFY.md).
