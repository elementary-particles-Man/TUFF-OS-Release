#!/bin/bash
set -euo pipefail

# KAIRO-FW Debian Package Builder
# Version: 0.3.0-1

BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KAIRO_ROOT="$(cd "${BASE_DIR}/../TUFF-KAIRO" && pwd)"
DEBIAN_STAGING="${BASE_DIR}/debian"
DEB_OUT_DIR="${BASE_DIR}/deb"

# Release Architecture: amd64 only
ARCH="amd64"
TARGET="x86_64-unknown-linux-gnu"
LINKER="x86_64-linux-gnu-gcc"
TOOLCHAIN="+1.85.0"

echo "Building KAIRO-FW 0.3.0-1 Debian package for ${ARCH}..."

# 1. Build Real KAIRO CUI (AMD64)
echo "Building real KAIRO CUI for ${ARCH}..."
(cd "${KAIRO_ROOT}" && \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="${LINKER}" \
    cargo ${TOOLCHAIN} build --release -p kairo-cli --target "${TARGET}")

BINARY_PATH="${KAIRO_ROOT}/target/${TARGET}/release/kairo-cli"
if [ ! -f "${BINARY_PATH}" ]; then
    echo "ERROR: Real KAIRO CUI binary not found at ${BINARY_PATH}"
    exit 1
fi

# 2. Setup Staging Area
rm -rf "${DEBIAN_STAGING}"
mkdir -p "${DEBIAN_STAGING}/DEBIAN"
mkdir -p "${DEBIAN_STAGING}/usr/bin"
mkdir -p "${DEBIAN_STAGING}/usr/share/kairo-fw/rules"
mkdir -p "${DEBIAN_STAGING}/usr/share/kairo-fw/docs"
mkdir -p "${DEBIAN_STAGING}/usr/share/doc/kairo-fw"

# Control file
cat > "${DEBIAN_STAGING}/DEBIAN/control" <<EOF
Package: kairo-fw
Version: 0.3.0-1
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: elementary-particles-Man
Description: KAIRO CUI operational package / rule and posture verification surface.
 This package provides the real KAIRO Management CLI (kairo) backed by the
 core/net/sec stack. It is used for rule, posture, and security verification.
 Note: This release does not include the ethics engine and does not claim
 production kernel-level firewall enforcement.
EOF

# Install Binary
cp "${BINARY_PATH}" "${DEBIAN_STAGING}/usr/bin/kairo"
chmod 0755 "${DEBIAN_STAGING}/usr/bin/kairo"

# Install Rules
cp "${KAIRO_ROOT}/rules/kairo-cve-response.toml" "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/"
cp "${KAIRO_ROOT}/rules/kairo-non-cve-threat-response.toml" "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/"
mkdir -p "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/linux"
cp "${KAIRO_ROOT}/rules/linux/"*.toml "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/linux/"
mkdir -p "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/ai-agent"
cp "${KAIRO_ROOT}/rules/ai-agent/"*.toml "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/ai-agent/"
mkdir -p "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/kubernetes"
cp "${KAIRO_ROOT}/rules/kubernetes/"*.toml "${DEBIAN_STAGING}/usr/share/kairo-fw/rules/kubernetes/"

# Install Documentation
cp "${BASE_DIR}/README.md" "${DEBIAN_STAGING}/usr/share/kairo-fw/README.md"
cp "${BASE_DIR}/CHANGELOG.md" "${DEBIAN_STAGING}/usr/share/kairo-fw/CHANGELOG.md"
cp "${BASE_DIR}/docs/"*.md "${DEBIAN_STAGING}/usr/share/kairo-fw/docs/"
echo "0.3.0" > "${DEBIAN_STAGING}/usr/share/kairo-fw/VERSION"

# Changelog
cat > "${DEBIAN_STAGING}/usr/share/doc/kairo-fw/changelog.Debian" <<EOF
kairo-fw (0.3.0-1) unstable; urgency=medium

  * Release 0.3.0.
  * Replaced placeholder wrapper with real KAIRO CUI binary.
  * Integrated real kairo-core, kairo-net, and kairo-sec stack.
  * Bundled security rules and metadata.
  * Explicitly excluded ethics engine from package payload.

 -- elementary-particles-Man <$(whoami)@$(hostname)>  $(date -R)
EOF

# Ethics engine and legacy exclusion check
echo "Running payload exclusion check..."
# Fail if any restricted word is found in the staged binary or metadata (ignoring docs/changelogs/control)
if find "${DEBIAN_STAGING}" -type f \
    ! -path "*/usr/share/kairo-fw/docs/*" \
    ! -path "*/usr/share/doc/kairo-fw/*" \
    ! -name "README.md" \
    ! -name "CHANGELOG.md" \
    ! -name "control" \
    -exec grep -Ei "ethics_engine|KAIRO-OLD|kairo-daemon" {} +; then
    echo "ERROR: Restricted payload detected in staging tree!"
    exit 1
fi
echo "Exclusion check passed."

# Build package
mkdir -p "${DEB_OUT_DIR}"
DEB_FILE="${DEB_OUT_DIR}/kairo-fw_0.3.0-1_${ARCH}.deb"
dpkg-deb --build "${DEBIAN_STAGING}" "${DEB_FILE}"

# Generate SHA256SUMS
(cd "${DEB_OUT_DIR}" && sha256sum kairo-fw_0.3.0-1_*.deb > SHA256SUMS)

echo "Build complete: ${DEB_FILE}"
