cd aur
for hash in $(git rev-list --max-count=10 HEAD); do
  echo "Checking $hash..."
  git ls-tree --name-only -r $hash | grep -q '^PKGBUILD$' || echo "❌ Missing PKGBUILD in $hash"
done
makepkg --printsrcinfo > .SRCINFO
cd ..
