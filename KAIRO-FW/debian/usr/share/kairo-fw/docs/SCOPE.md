# KAIRO-FW 0.3.0 Technical Scope

This document defines what is included in the KAIRO-FW 0.3.0 release and sets the boundaries for its current role.

## Included in this Release

- **Real KAIRO Management Tool:** The `/usr/bin/kairo` binary built from the real KAIRO management stack.
- **Rule Metadata:** Necessary data for verifying system security rules and postures, packaged locally.
- **Documentation:** Bundled user and technical guides for the real tool.
- **Deployment Format:** Standardized Debian package for easy installation on supported architectures (amd64).

## Not in Scope for this Release

- **Active Enforcement:** This release does not include logic for automatically blocking network traffic or enforcing firewall rules in the kernel.
- **Network Filtering:** This package is not currently a replacement for system firewalls like `iptables` or `nftables`.
- **Ethics Policy Runtime:** The ethics engine and its related modules are strictly excluded from this package payload.
- **Retired Components:** Legacy `kairo-daemon` and `KAIRO-OLD` artifacts are not included.

## Technical Boundary

KAIRO-FW 0.3.0 provides the **actual operational surface** for the KAIRO ecosystem. It bundles the real `kairo` CLI which directly interfaces with the core security logic, threat detection, and posture evaluation components (`kairo-sec`). KAIRO-FW acts as the delivery vehicle and operational interface for these production-ready components.
