for hash in $(git rev-list --max-count=10 HEAD); do
  echo "Checking $hash..."
  git ls-tree --name-only -r $hash | grep -q '^PKGBUILD$' || echo "❌ Missing PKGBUILD in $hash"
done
<<<<<<< HEAD
makepkg --printsrcinfo > .SRCINFO
=======
makepkg --printsrcinfo > .SRCINFO
>>>>>>> dab388da8e452a4f82f1297bf20c48254028d1c1
