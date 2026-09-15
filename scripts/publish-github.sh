#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

command -v gh >/dev/null || { echo "GitHub CLI (gh) is required." >&2; exit 1; }
gh auth status >/dev/null
OWNER="$(gh api user --jq .login)"
REPO="g13-nexus"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"

./scripts/build-rpm.sh

# User explicitly requested repository recreation.
if gh repo view "$OWNER/$REPO" >/dev/null 2>&1; then
  gh repo delete "$OWNER/$REPO" --yes
fi

gh repo create "$OWNER/$REPO" \
  --public \
  --description "Native Linux configuration, remapping, macro, LCD, RGB, and joystick suite for the Logitech G13" \
  --source=. \
  --remote=origin

git init -b main >/dev/null 2>&1 || true
git add .
git -c user.name="$OWNER" -c user.email="${OWNER}@users.noreply.github.com" commit -m "G13 Nexus v${VERSION}" || true
git branch -M main
git remote set-url origin "https://github.com/$OWNER/$REPO.git"
git push -u origin main --force

git tag -f "v${VERSION}"
git push origin "v${VERSION}" --force

mapfile -t ASSETS < <(find dist -maxdepth 1 -type f \( -name '*.rpm' -o -name 'SHA256SUMS' \) -print | sort)
gh release create "v${VERSION}" "${ASSETS[@]}" \
  --repo "$OWNER/$REPO" \
  --title "G13 Nexus ${VERSION}" \
  --notes-file RELEASE_NOTES.md

echo "Published: https://github.com/$OWNER/$REPO/releases/tag/v${VERSION}"
