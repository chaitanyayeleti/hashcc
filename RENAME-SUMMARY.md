# Rename Summary: hashsum → hashcc

## Reason
Renamed to avoid conflict with coreutils-rs `hashsum` implementation.

## What Changed

### ✅ Completed
1. **GitHub repository**: Renamed to `hashcc`
2. **Cargo.toml**: Package name changed to `hashcc`
3. **src/main.rs**: Binary name and examples updated
4. **PKGBUILD files**: All updated for `hashcc` and `hashcc-git`
5. **README.md**: All references updated
6. **.gitignore**: Fixed (removed `/src` exclusion)
7. **Git remote**: Updated to point to new repo URL

### 🔄 To Do (when network is available)

#### 1. Delete old AUR package
Visit: https://aur.archlinux.org/packages/hashsum-git
- Click "Delete Package" (or mark as orphaned)

#### 2. Publish new AUR package (`hashcc-git`)
```bash
cd /home/yeleticc/learn/rustlea/stable
git clone ssh://aur@aur.archlinux.org/hashcc-git.git
cd hashcc-git
cp ~/learn/rustlea/hashsum/PKGBUILD-aur PKGBUILD
cp ~/learn/rustlea/hashsum/.SRCINFO-aur .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Initial import: hashcc-git (renamed from hashsum-git)"
git push
```

#### 3. (Optional) Publish stable package (`hashcc`)
First, create a new GitHub release:
- Go to: https://github.com/chaitanyayeleti/hashcc/releases/new
- Tag: `v0.1.0`
- Title: `v0.1.0 - Initial Release (renamed from hashsum)`
- Publish

Then update the stable PKGBUILD checksum:
```bash
curl -sL https://github.com/chaitanyayeleti/hashcc/archive/v0.1.0.tar.gz | sha256sum
# Update sha256sums in PKGBUILD-aur-stable with the output
```

Finally, publish to AUR:
```bash
cd /home/yeleticc/learn/rustlea/stable
git clone ssh://aur@aur.archlinux.org/hashcc.git
cd hashcc
cp ~/learn/rustlea/hashsum/PKGBUILD-aur-stable PKGBUILD
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Initial import: hashcc 0.1.0"
git push
```

## New Installation Commands

### For users:
```bash
# AUR (development version)
yay -S hashcc-git

# AUR (stable, once published)
yay -S hashcc

# From source
cargo install --git https://github.com/chaitanyayeleti/hashcc --features progress,archives
```

### Usage:
```bash
hashcc generate /path/to/file
hashcc verify checksums.csv --algo sha256
hashcc compare <hash> /path/to/file
```

## Files Ready for AUR
- `PKGBUILD-aur` → For `hashcc-git` package
- `.SRCINFO-aur` → Metadata for `hashcc-git`
- `PKGBUILD-aur-stable` → For `hashcc` stable package
- `.SRCINFO-aur-stable` → Metadata for `hashcc` (needs checksum update)

## Repository URLs
- **GitHub**: https://github.com/chaitanyayeleti/hashcc
- **AUR (git)**: https://aur.archlinux.org/packages/hashcc-git (to be published)
- **AUR (stable)**: https://aur.archlinux.org/packages/hashcc (optional)
