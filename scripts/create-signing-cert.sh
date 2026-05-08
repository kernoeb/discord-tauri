#!/usr/bin/env bash
set -euo pipefail

NAME="${1:-discord-tauri local}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-certificate -c "$NAME" >/dev/null 2>&1; then
  echo "cert '$NAME' already exists in keychain — nothing to do"
  exit 0
fi

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# Self-signed cert config with code signing EKU
cat > "$TMPDIR/openssl.cnf" <<EOF
[req]
distinguished_name = dn
prompt = no
[dn]
CN = $NAME
[v3_ca]
basicConstraints = CA:FALSE
extendedKeyUsage = codeSigning
keyUsage = digitalSignature
EOF

openssl genrsa -out "$TMPDIR/key.pem" 2048 2>/dev/null
openssl req -new -x509 -days 3650 -key "$TMPDIR/key.pem" \
  -out "$TMPDIR/cert.pem" \
  -config "$TMPDIR/openssl.cnf" -extensions v3_ca 2>/dev/null
# macOS `security` import only understands legacy PKCS12 algorithms.
openssl pkcs12 -export -legacy -out "$TMPDIR/cert.p12" \
  -inkey "$TMPDIR/key.pem" -in "$TMPDIR/cert.pem" \
  -password pass:dt 2>/dev/null

# Import into login keychain. -T grants access to codesign without prompts.
security import "$TMPDIR/cert.p12" -k "$KEYCHAIN" -P "dt" \
  -T /usr/bin/codesign \
  -T /usr/bin/security

# Allow codesign to use the private key without keychain prompts.
# This needs the user's login password — security will prompt once.
security set-key-partition-list -S apple-tool:,apple:,codesign: \
  -s -k "" "$KEYCHAIN" 2>/dev/null || \
  security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s "$KEYCHAIN"

echo "cert '$NAME' created and imported into login keychain"
echo "next: rebuild with 'make bundle-macos' and reset existing TCC entries in System Settings"
