# hashcc

A fast, parallel hashing utility for Linux supporting multiple algorithms with security features and ecosystem compatibility.

## Features

- **Multiple algorithms**: MD5, SHA-1, SHA-256, SHA-512, BLAKE3
- **Parallel processing**: Uses Rayon for multi-threaded directory hashing
- **Memory-efficient**: mmap for large files, streaming for others
- **Security hardened**:
  - Constant-time hash comparison
  - Weak algorithm warnings (MD5, SHA-1)
  - Path traversal protection with `--base-dir`
- **sha256sum compatible**: Read/write standard sumfiles
- **Optional features**:
  - Progress bars (`--progress`)
  - Archive hashing (`--archives`) for .zip, .tar, .tar.gz
- **Multiple output formats**: text, JSON, CSV, sumfile

## Installation

### Arch Linux (AUR)
```bash
yay -S hashcc-git
# or
paru -S hashcc-git
```

### From source
```bash
cargo install --git https://github.com/chaitanyayeleti/hashcc --features progress,archives
```

### Build locally
```bash
git clone https://github.com/chaitanyayeleti/hashcc
cd hashcc
cargo build --release --features progress,archives
sudo install -Dm755 target/release/hashcc /usr/local/bin/hashcc
```

## Usage

### Generate hashes
```bash
# Single file (SHA-256 default)
hashcc generate /path/to/file

# Directory with CSV output
hashcc generate --format csv /path/to/dir > checksums.csv

# With progress and archive support
hashcc generate --progress --archives --format json /path/to/dir

# Exclude patterns
hashcc generate --exclude '**/*.tmp' --exclude 'node_modules/**' /path/to/dir

# stdin
echo -n 'hello' | hashcc generate --algo blake3
```

### Verify hashes
```bash
# Verify CSV
hashcc verify checksums.csv --algo sha256 --base-dir /path/to/dir

# Verify sha256sum-style sumfile
hashcc verify SHA256SUMS --sumfile --algo sha256 --base-dir /path/to/dir
```

### Compare hash
```bash
hashcc compare <EXPECTED_HASH> /path/to/file --algo sha256
```

## Output Formats

- `--format text`: `<hash>  <path>` (default)
- `--format json`: JSON array of `{path, hash}`
- `--format csv`: CSV with header `path,hash`
- `--format sumfile`: sha256sum-compatible format

## Security

- Refuses weak algorithms (MD5, SHA-1) by default; override with `--allow-weak`
- Constant-time hash comparison prevents timing attacks
- Path validation prevents directory traversal when using `--base-dir`
- Absolute paths blocked by default in verify; enable with `--allow-absolute`

## Build Features

- `progress`: Progress bars with indicatif (optional)
- `archives`: Hash inside .zip and .tar archives (optional)

Default build is lean. Enable features:
```bash
cargo build --release --features progress,archives
```

## Examples

Generate and verify workflow:
```bash
# Generate checksums
hashcc generate --format sumfile /data > SHA256SUMS

# Later, verify
hashcc verify SHA256SUMS --sumfile --base-dir /data
```

Archive hashing:
```bash
# Hash contents of archives
hashcc generate --archives --format json /backups
# Output includes virtual paths like: backup.tar.gz!/inner/file.txt
```

## License

MIT OR Apache-2.0

## Author

Chaitanya Yeleti <chaitanyachowdary125@live.com>
