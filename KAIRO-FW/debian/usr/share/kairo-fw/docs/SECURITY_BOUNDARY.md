# KAIRO-FW Security Boundary

This document defines the security principles, exclusions, and operational boundaries for the `kairo-fw` package.

## Core Principles

- **No Implicit Enforcement:** This package is restricted to an operational and verification role. It does not silently gain network enforcement privileges or modify system firewall configurations without explicit, audited, and separately versioned updates.
- **Fail-Closed Verification:** All rule and posture checks provided by the packaged metadata are designed to fail closed if validation fails, ensuring a secure reporting state.
- **Transparency:** The package bundles the real KAIRO stack to ensure that users are interacting with the actual production-ready operational surface.

## Security Exclusions

To maintain a clean security perimeter, the following components are strictly excluded from the `kairo-fw` 0.3.0 payload:

- **Ethics Engine:** No ethics policy runtime or related modules are included.
- **Exploit Logic:** No exploit code, trigger logic, or PoC code is permitted in this package.
- **Credential Capture:** The package does not perform or support any form of credential harvesting or capture.
- **Legacy Payloads:** No artifacts from the retired `kairo-daemon` or the legacy `KAIRO-OLD` repository paths are included.

## Operational Security

- **Real Stack Integration:** The current release provides the real `kairo` CLI backed by `kairo-core`, `kairo-net`, and `kairo-sec`.
- **Passive Posture:** While the stack is real, its role in this release is restricted to passive verification and auditing.
- **Audit Logging:** Interaction with system paths for verification is handled via the integrated KAIRO secure logging and audit mechanisms.
