# KAIRO-FW

KAIRO-FW is the KAIRO ecosystem packaging surface for the Debian baseline of the firewall-related component.

## 0.2.0 Debian baseline

This repository snapshot carries the `0.2.0` packaging baseline, not a production firewall enforcement stack.

- Package: `kairo-fw`
- Version: `0.2.0-1`
- Architecture: `all`
- Debian artifact: `KAIRO-FW/deb/kairo-fw_0.2.0_all.deb`
- Checksum manifest: `KAIRO-FW/deb/SHA256SUMS`

The package is intentionally documentation and metadata focused. It establishes the release structure and verification path, but it does not claim kernel-level or runtime firewall enforcement.

## Verification

From the repository root:

```bash
cat KAIRO-FW/VERSION
(cd KAIRO-FW/deb && sha256sum -c SHA256SUMS)
dpkg-deb --info KAIRO-FW/deb/kairo-fw_0.2.0_all.deb
```

Expected results:

- `KAIRO-FW/VERSION` reports `0.2.0`
- `sha256sum -c` reports `OK`
- `dpkg-deb --info` shows `Package: kairo-fw`, `Version: 0.2.0-1`, and `Architecture: all`

## Legacy package preservation

The older `0.1.0` package remains preserved as `KAIRO-FW/kairo-fw_0.1.0-1_amd64.deb`.
It is kept for reference and rollback comparison and is not replaced by the new baseline artifact.

## Scope

The 0.2.0 package baseline documents the separation of responsibility between the boot domains:

- `BOOT-TUFF` remains the TUFF-side boot and installation path.
- `BOOT-RADICAL` remains the RADICAL-side boot and host posture path.

KAIRO-FW packaging should stay aligned with that split. This README documents the packaging boundary only and does not redefine the operational responsibilities of either boot path.
