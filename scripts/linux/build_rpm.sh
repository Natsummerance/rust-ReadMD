#!/usr/bin/env bash
# ReadMD RPM 打包脚本 (适用于 openEuler / Fedora / RHEL / CentOS / Anolis / openSUSE)
set -euo pipefail
if [ -z "${READMD_VERSION:-}" ]; then
  if [ -f "VERSION" ]; then
    READMD_VERSION="$(head -n1 VERSION | tr -d '\r\n')"
  else
    echo 'ERROR: VERSION is required' >&2; exit 2
  fi
fi
VERSION="${READMD_BUILD_VERSION:-${READMD_VERSION}}"
ARCH="$(uname -m)"
RPM_ARCH="${READMD_RPM_ARCH:-}"
if [ -z "${RPM_ARCH}" ]; then
  case "${ARCH}" in
    aarch64|arm64) RPM_ARCH="aarch64" ;;
    x86_64|amd64)  RPM_ARCH="x86_64" ;;
    *) echo "ERROR: no verified native target for ${ARCH}" >&2; exit 2 ;;
  esac
fi

echo "=== Building ReadMD v${VERSION} RPM Package (${ARCH} / ${RPM_ARCH}) ==="

case "${RPM_ARCH}" in x86_64|aarch64) ;; *) echo 'ERROR: unsupported RPM architecture' >&2; exit 2 ;; esac
command -v file >/dev/null 2>&1 || { echo 'ERROR: file is required to verify the ELF architecture' >&2; exit 2; }
BIN_INFO="$(file dist/ReadMD/ReadMD)"
case "${RPM_ARCH}" in
  x86_64) echo "${BIN_INFO}" | grep -Eqi 'ELF.*(x86-64|x86_64)' || { echo 'ERROR: RPM target does not match ELF architecture' >&2; exit 2; } ;;
  aarch64) echo "${BIN_INFO}" | grep -Eqi 'ELF.*(aarch64|ARM aarch64)' || { echo 'ERROR: RPM target does not match ELF architecture' >&2; exit 2; } ;;
esac

BUILD_ROOT="dist/rpm_build"
rm -rf "${BUILD_ROOT}"
mkdir -p "${BUILD_ROOT}/BUILD" "${BUILD_ROOT}/RPMS" "${BUILD_ROOT}/SOURCES" "${BUILD_ROOT}/SPECS" "${BUILD_ROOT}/SRPMS"
mkdir -p "${BUILD_ROOT}/BUILDROOT/readmd-${VERSION}-1.${RPM_ARCH}"

ROOT_DIR="${BUILD_ROOT}/BUILDROOT/readmd-${VERSION}-1.${RPM_ARCH}"
mkdir -p "${ROOT_DIR}/opt/readmd" "${ROOT_DIR}/usr/bin" "${ROOT_DIR}/usr/share/applications" "${ROOT_DIR}/usr/share/icons/hicolor/512x512/apps" "${ROOT_DIR}/usr/share/mime/packages"

# 复制编译生成的二进制文件
if [ -f "dist/ReadMD/ReadMD" ] && [ -f "dist/ReadMD/assets/index.html" ]; then
  cp -r dist/ReadMD/* "${ROOT_DIR}/opt/readmd/"
else
  echo "Error: dist/ReadMD does not exist. Please run cargo build --release first." >&2
  exit 1
fi

ln -sf /opt/readmd/ReadMD "${ROOT_DIR}/usr/bin/readmd"
cp scripts/linux/io.github.natsummerance.readmd.desktop "${ROOT_DIR}/usr/share/applications/"
cp assets/ReadMD.png "${ROOT_DIR}/usr/share/icons/hicolor/512x512/apps/readmd.png"
cp scripts/linux/readmd.xml "${ROOT_DIR}/usr/share/mime/packages/"

# 生成 RPM SPEC 文件
cat << EOF > "${BUILD_ROOT}/SPECS/readmd.spec"
Name:           readmd
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Lightweight Markdown viewer and editor with auto-repair and multi-platform support
License:        MIT
URL:            https://readmd.asia
BuildArch:      ${RPM_ARCH}

# The binary links GTK3/WebKitGTK 4.1. A browser is not a native fallback.
Requires:       glibc, gtk3, webkit2gtk4.1, xdg-utils
Recommends:     zenity, tesseract, google-noto-sans-cjk-fonts

%description
ReadMD is a lightweight, privacy-first native Markdown reader, editor and document
converter with formula rendering, presentation mode, auto-repair and multi-architecture support.

%files
/opt/readmd/*
/usr/bin/readmd
/usr/share/applications/io.github.natsummerance.readmd.desktop
/usr/share/icons/hicolor/512x512/apps/readmd.png
/usr/share/mime/packages/readmd.xml

%post
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
  update-mime-database /usr/share/mime || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
chmod +x /opt/readmd/ReadMD 2>/dev/null || true

%postun
if [ \$1 -eq 0 ]; then
  rm -f /usr/bin/readmd
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
  fi
  if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database /usr/share/mime || true
  fi
fi

EOF

# 执行 rpmbuild
if command -v rpmbuild >/dev/null 2>&1; then
  rpmbuild --define "_topdir $(pwd)/${BUILD_ROOT}" -bb "${BUILD_ROOT}/SPECS/readmd.spec"
  cp "${BUILD_ROOT}/RPMS/${RPM_ARCH}/readmd-${VERSION}-1."*".rpm" "dist/readmd-${VERSION}-1.${RPM_ARCH}.rpm" || true
  echo "=== RPM package built: dist/readmd-${VERSION}-1.${RPM_ARCH}.rpm ==="
else
  echo "rpmbuild not available on this host; spec file prepared in ${BUILD_ROOT}/SPECS/readmd.spec"
fi
