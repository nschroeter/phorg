# phorg

Organizes photos into a `YYYY/MM/DD` folder structure based on EXIF date.

## Usage

```
phorg <src> <dest> [--dry-run]
```

`--dry-run` prints what would be moved without making any changes.

Moves all `.ARW`, `.JPG`, and `.JPEG` files found recursively in `<src>` into `<dest>/YYYY/MM/DD/`.

## Behavior

- Files without an EXIF `DateTimeOriginal` tag are skipped (logged to stderr)
- Darktable `.xmp` sidecar files (e.g. `A1_0001.ARW.xmp`) are moved alongside their photo if present
- Duplicate files (same content) at the destination are skipped
- Filename conflicts (same name, different content) are renamed: `A1_0001(1).ARW`, `A1_0001(2).ARW`, etc.
- Empty source directories are removed after the import
- `<dest>` must not be inside `<src>`

## Supported formats

- Sony ARW (RAW)
- JPEG / JPG

## Building

```
cargo build --release
```

Binary: `target/release/phorg`

### Linux (from macOS)

Requires [Docker](https://www.docker.com/):

```
docker run --rm --platform linux/amd64 \
  -v $(pwd):/app \
  -v ~/.cargo/registry:/usr/local/cargo/registry \
  -w /app rust:latest \
  cargo build --release
```

## Running tests

```
cargo test
```
