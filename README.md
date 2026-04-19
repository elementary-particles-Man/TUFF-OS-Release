# TUFF-OS LiveUSB Repository

This repository contains the TUFF-OS LiveUSB installation images and release artifacts.

## Status
- **VM Testing**: Complete (All tests passed in QEMU environment).
- **Bare-Metal Testing**: UNCONFIRMED (Not yet tested on physical hardware).

## Disclaimer
**USE AT YOUR OWN RISK.** TUFF-OS interfaces directly with the physical layer. The developers are not responsible for any data loss or hardware damage.

## About TUFF-OS
The Ultimate Fortress Foundation OS (TUFF-OS) is a security-focused OS that provides absolute data sovereignty at the physical layer.

## Distribution
- **Version**: 1.1.0 (LiveUSB Installer Release)
- **Target**: x86_64 UEFI Secure Boot compliant
- **Latest Image**: `latest/TUFF-OS-latest.iso`
- **Image Format**: ISO image for LiveUSB write and UEFI boot
- **Image Generation**: Release images are produced with the standard release tool and are not hand-assembled or manually edited.

## Using the ISO
1. Download the matching set from the same commit:
   - `latest/TUFF-OS-latest.iso`
   - `latest/TUFF-OS-latest.iso.sigv1`
   - `latest/update-metadata.json`
2. Write `latest/TUFF-OS-latest.iso` directly to a USB drive. Do not extract the ISO and do not copy its files manually.
3. Recommended writing tools:
   - Linux: `sudo dd if=latest/TUFF-OS-latest.iso of=/dev/sdX bs=4M status=progress oflag=sync`
   - Windows: Rufus or balenaEtcher in raw image write mode
4. Boot the target machine in UEFI mode from that USB drive and follow the installer flow.
5. Keep the `.sigv1` file and `update-metadata.json` together with the ISO when mirroring or redistributing the release.

## Applying Differential Updates
- Differential packages are published under `deltas/*.tpatch`.
- End users should not unpack or regenerate `.tpatch` files by hand. On an installed TUFF-OS system, run `tuffutl TUFFOS update`.
- The updater reads `latest/update-metadata.json`, selects the matching `deltas/v<from>_to_v<to>.tpatch`, verifies `patch_sig`, applies the patch, and then verifies the rebuilt result against `full_iso_sig`.
- If no matching delta exists, the updater falls back to the full ISO path.
- If signature verification fails, abort the update immediately and re-download the matching release set from this repository. Do not mix files from different commits.

## Development Support & Source Code Access
TUFF-OS is currently developed and maintained as an independent project. To support the continued development and testing on physical hardware, we humbly request your assistance.

- **Support via Wishlist**: If you find this project valuable, please consider supporting us through our [Amazon Wishlist](https://www.amazon.jp/hz/wishlist/ls/3NB2B9PB5XJ3I?ref_=wl_share). Your support is deeply appreciated and will directly contribute to the project's growth.
- **Source Code Access**: The source code is currently available to a limited audience. If you wish to access the source code for research or collaborative purposes, please reach out to us with your Git account information. We would be honored to share it with you privately.

## License
- Free for Individual, Research, and Non-Commercial use.
- Commercial use requires an individual contract.
See [LICENSE](LICENSE) for details.

---

## Third-Party API and Trademark Notice

This product may use a compute backend built on the Vulkan API where available.

Vulkan is a registered trademark of The Khronos Group Inc.

The Khronos Group Inc. is not the developer, vendor, maintainer, support provider, certifier, or end-user operator of this product. The Khronos Group Inc. does not endorse this product, and is not responsible for this product’s design, implementation, operation, maintenance, support, safety, security, regulatory status, or any direct or indirect results arising from its use.

Any optional Vulkan-based backend in this product is provided solely by this product’s own developers and distributors. Unless explicitly stated otherwise under separately satisfied Khronos requirements, this product does not claim Khronos conformance, Khronos compliance, Khronos certification, official Vulkan support status, or any other Khronos approval.

For license and attribution details regarding any redistributed third-party components, see `THIRD_PARTY_NOTICES.md` and `TRADEMARKS.md`.
