# KAIRO+PAL P0 RC0 Debian Package

Status: pre-hardware-test.

This directory contains the KAIRO+PAL P0 RC0 Debian package artifacts staged for validation. Physical end-to-end testing is still pending because the test machine memory replacement is not complete.

## Included

- KAIRO+PAL P0 security package artifacts
- Debian package artifact(s)
- SHA256SUMS
- manifest.json

## Not Included / Non-goals

- Not stable
- Not production-ready
- Not promoted to latest
- No live install was performed during artifact staging
- No live exploit reproduction or external probing was performed

## Verification

Use sha256sum -c SHA256SUMS in this directory.

## Known Blocker

The known full-test blocker remains the pre-existing vulkan_gpu::tests::prefilter_invokes_tuff_gpgpu_and_returns_result SIGSEGV outside this packaging task.
