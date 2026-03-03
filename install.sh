#!/usr/bin/env bash
# Install peek and peekd from GitHub Releases.
# Linux only. Requires curl.
set -euo pipefail

VERSION="${PEEK_VERSION:-1.0.0}"
TAG="v${VERSION}"
VERSION_LABEL="v${VERSION%.*}"
INSTALL_DIR="${PEEK_INSTALL_DIR:-/usr/local/bin}"
REPO="${PEEK_REPO:-ankittk/peek}"

case "$(uname -s)" in
  Linux) ;;
  *)
    echo "This installer is for Linux only. On macOS use: brew install peek" >&2
    exit 1
    ;;
esac

ARCH=$(uname -m)
case "$ARCH" in
  x86_64)     ASSET_SUFFIX="x86_64-linux-musl" ;;
  aarch64|arm64) ASSET_SUFFIX="aarch64-linux-musl" ;;
  *)
    echo "Unsupported architecture: $ARCH (supported: x86_64, aarch64)" >&2
    exit 1
    ;;
esac

PEEK_FILE="peek-${VERSION_LABEL}-${ASSET_SUFFIX}"
PEEKD_FILE="peekd-${VERSION_LABEL}-${ASSET_SUFFIX}"
PEEK_URL="https://github.com/${REPO}/releases/download/${TAG}/${PEEK_FILE}"
PEEKD_URL="https://github.com/${REPO}/releases/download/${TAG}/${PEEKD_FILE}"
CHECKSUMS_URL="https://github.com/${REPO}/releases/download/${TAG}/checksums.sha256"

echo "Installing peek ${VERSION} (${ASSET_SUFFIX}) to ${INSTALL_DIR}..."

if ! command -v sha256sum >/dev/null 2>&1; then
  echo "sha256sum is required for checksum verification. Please install coreutils and try again." >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading checksums..."
curl -sSL -o "${TMP_DIR}/checksums.sha256" "$CHECKSUMS_URL"

echo "Downloading binaries..."
curl -sSL -o "${TMP_DIR}/${PEEK_FILE}" "$PEEK_URL"
curl -sSL -o "${TMP_DIR}/${PEEKD_FILE}" "$PEEKD_URL"

echo "Verifying checksums..."
(
  cd "$TMP_DIR"
  sha256sum -c --ignore-missing checksums.sha256
)

mkdir -p "$INSTALL_DIR"
install -m 0755 "${TMP_DIR}/${PEEK_FILE}" "${INSTALL_DIR}/peek"
install -m 0755 "${TMP_DIR}/${PEEKD_FILE}" "${INSTALL_DIR}/peekd"

echo "peek ${VERSION} and peekd installed to ${INSTALL_DIR}"
echo "Ensure ${INSTALL_DIR} is on your PATH."

# Optionally install systemd unit so "sudo systemctl start peekd" works
INSTALL_SYSTEMD="${PEEK_INSTALL_SYSTEMD:-}"
if [ -z "$INSTALL_SYSTEMD" ] && [ -t 0 ]; then
  echo -n "Install systemd unit for peekd? [y/N] "
  read -r ans
  [ "$ans" = "y" ] || [ "$ans" = "Y" ] && INSTALL_SYSTEMD=1
fi
if [ -n "$INSTALL_SYSTEMD" ]; then
  SVC_DIR="/etc/systemd/system"
  SVC_PATH="${SVC_DIR}/peekd.service"
  PEEKD_BIN="${INSTALL_DIR}/peekd"
  UNIT_SRC=""
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd)"
  if [ -n "$SCRIPT_DIR" ] && [ -f "${SCRIPT_DIR}/packaging/peekd.service" ]; then
    UNIT_SRC="${SCRIPT_DIR}/packaging/peekd.service"
  elif [ -f "packaging/peekd.service" ]; then
    UNIT_SRC="packaging/peekd.service"
  else
    UNIT_TMP=$(mktemp)
    if curl -sSL "https://raw.githubusercontent.com/${REPO}/main/packaging/peekd.service" -o "$UNIT_TMP" 2>/dev/null; then
      UNIT_SRC="$UNIT_TMP"
    fi
  fi
  if [ -n "$UNIT_SRC" ]; then
    if [ -w "$SVC_DIR" ] 2>/dev/null; then
      sed "s|ExecStart=.*|ExecStart=${PEEKD_BIN}|" "$UNIT_SRC" > "$SVC_PATH"
      [ -n "${UNIT_TMP:-}" ] && rm -f "$UNIT_TMP"
      systemctl daemon-reload
      echo "Installed ${SVC_PATH}. Run: sudo systemctl start peekd && sudo systemctl enable peekd"
    else
      echo "To install the systemd unit run:"
      echo "  (sed 's|ExecStart=.*|ExecStart=${PEEKD_BIN}|' $UNIT_SRC | sudo tee $SVC_PATH) && sudo systemctl daemon-reload"
    fi
  else
    echo "To install the systemd unit: copy packaging/peekd.service to ${SVC_PATH}, set ExecStart=${PEEKD_BIN}, then sudo systemctl daemon-reload && sudo systemctl start peekd"
  fi
else
  echo "For peekd as a service: copy packaging/peekd.service to /etc/systemd/system/ (set ExecStart to ${INSTALL_DIR}/peekd if not /usr/bin/peekd), then systemctl daemon-reload && systemctl start peekd"
fi
