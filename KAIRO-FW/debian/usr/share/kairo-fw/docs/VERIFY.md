# KAIRO-FW Release Verification

This guide explains how to verify that your `kairo-fw` package is authentic and correctly installed.

## User Verification

After downloading the package, you can run these commands to check its integrity.

### 1. Check File Integrity
Verify that the package file hasn't been tampered with:
```sh
sha256sum -c SHA256SUMS
```
*Expected Output:* `kairo-fw_0.3.0-1_amd64.deb: OK`

### 2. Verify Package Identity
Ensure the package contains the correct version and description:
```sh
dpkg-deb --info kairo-fw_0.3.0-1_amd64.deb
```
*Look for:*
- `Package: kairo-fw`
- `Version: 0.3.0-1`
- `Architecture: amd64`
- `Description: KAIRO CUI operational package / rule and posture verification surface.`

### 3. Confirm Correct Installation
After installing, you can verify that the real tool is functional:
```sh
kairo status
kairo security matrix status
```

## Advanced/Maintainer Verification

If you want to manually inspect the package contents without installing it, use the following steps.

### Inspect Included Files
```sh
dpkg-deb --contents kairo-fw_0.3.0-1_amd64.deb
```
Confirm the following key files are present:
- `/usr/bin/kairo` (Should be an ELF binary, not a script)
- `/usr/share/kairo-fw/VERSION`
- `/usr/share/kairo-fw/rules/kairo-cve-response.toml`
- `/usr/share/kairo-fw/docs/USAGE.md`

### Verify Binary Type
Extract the package and run `file` on the binary:
```sh
dpkg-deb -x kairo-fw_0.3.0-1_amd64.deb /tmp/kairo-check
file /tmp/kairo-check/usr/bin/kairo
```
*Expected Result:* `ELF 64-bit LSB pie executable, x86-64...`

### Ensure Exclusion of Restricted Components
Confirm that no ethics engine components or legacy payloads are present:
```sh
dpkg-deb --contents kairo-fw_0.3.0-1_amd64.deb | grep -Ei 'ethics_engine|KAIRO-OLD|kairo-daemon'
```
*Expected Result:* No results (ignoring documentation mentions of exclusion).
