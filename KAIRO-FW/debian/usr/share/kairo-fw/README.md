# KAIRO-FW

KAIRO-FW 0.3.0 is a management package that provides the real KAIRO command-line tool to check your system's security posture and rule status. This version is backed by the actual KAIRO core, networking, and security stack.

## Quick Start

### 1. Install
Install the package on your Debian-based system (amd64):
```sh
sudo dpkg -i kairo-fw_0.3.0-1_amd64.deb
```

### 2. Run
Check your current status:
```sh
kairo status
```

View the security matrix status:
```sh
kairo security matrix status
```

Check for retired components:
```sh
kairo forbidden-scan
```

## What is included?
- **Real KAIRO Management Tool:** The actual `kairo` binary built from the TUFF-KAIRO source.
- **Security Rules:** Bundled rule definitions and metadata (CVE and non-CVE).
- **Core Stack:** Integrated KAIRO core, net, and sec components.
- **User Documentation:** Practical guides for using and verifying the real tool.

## Safety Boundary
- **Included:** Operational tool for status checks, rule auditing, and verification.
- **Not Included:** This release does not include the ethics engine, old daemon, or legacy components.
- **No Automatic Changes:** This package is for status checking and verification; it does not automatically change your system's firewall rules or kernel settings in this release.

## Detailed Information
For more specific details, please see:
- [User Manual](docs/USAGE.md): How to use the `kairo` tool.
- [Release Notes](docs/RELEASE_NOTES_0.3.0.md): What's new in this version.
- [Technical Scope](docs/SCOPE.md): What is and isn't covered by this release.
- [Security Boundary](docs/SECURITY_BOUNDARY.md): Detailed security principles.
- [Verification Guide](docs/VERIFY.md): Technical steps to verify package integrity.
