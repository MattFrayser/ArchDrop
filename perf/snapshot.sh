#!/usr/bin/env bash
set -euo pipefail

# --- Config ---
PERF_DIR="$(cd "$(dirname "$0")" && pwd)"
SNAPSHOTS_DIR="$PERF_DIR/snapshots"
BINARY_NAME="archdrop"
CARGO_ROOT="$(cd "$PERF_DIR/.." && pwd)"

# --- Validate args ---
if [ $# -lt 1 ]; then
    echo "Usage: $0 \"description of current state\""
    echo "Example: $0 \"baseline before buffer pool\""
    exit 1
fi

DESCRIPTION="$1"
SLUG=$(echo "$DESCRIPTION" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | sed 's/--*/-/g' | sed 's/^-//;s/-$//')
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
SHORT_HASH=$(git -C "$CARGO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "nogit")
SNAPSHOT_NAME="${TIMESTAMP}-${SHORT_HASH}-${SLUG}"
SNAPSHOT_DIR="$SNAPSHOTS_DIR/$SNAPSHOT_NAME"

mkdir -p "$SNAPSHOT_DIR"

echo "=== Performance Snapshot ==="
echo "Label:  $DESCRIPTION"
echo "Commit: $SHORT_HASH ($(git -C "$CARGO_ROOT" log -1 --format='%s' 2>/dev/null || echo 'unknown'))"
echo "Output: $SNAPSHOT_DIR"
echo ""

# --- Build release ---
echo "[1/5] Building release binary..."
cargo build --release --manifest-path "$CARGO_ROOT/Cargo.toml" 2>&1 | tail -1

BINARY="$CARGO_ROOT/target/release/$BINARY_NAME"
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

# --- Record perf ---
echo "[2/5] Starting perf record..."
echo "  >> Start your transfer scenario now."
echo "  >> Press Ctrl+C in the archdrop process when done."
echo ""

PERF_DATA="$SNAPSHOT_DIR/perf.data"
perf record -g --call-graph dwarf -o "$PERF_DATA" "$BINARY" "${@:2}" || true

if [ ! -f "$PERF_DATA" ]; then
    echo "ERROR: perf.data not generated"
    exit 1
fi

# --- Collapse stacks ---
echo ""
echo "[3/5] Collapsing stacks..."
perf script -i "$PERF_DATA" | ~/.cargo/bin/inferno-collapse-perf > "$SNAPSHOT_DIR/collapsed.txt" 2>/dev/null

# --- Generate flamegraph ---
echo "[4/5] Generating flamegraph..."
~/.cargo/bin/inferno-flamegraph < "$SNAPSHOT_DIR/collapsed.txt" > "$SNAPSHOT_DIR/flamegraph.svg" 2>/dev/null

# --- Generate summary ---
echo "[5/5] Generating summary..."

python3 -c "
total = 0
leaf_counts = {}

with open('$SNAPSHOT_DIR/collapsed.txt') as f:
    for line in f:
        line = line.strip()
        if not line: continue
        parts = line.rsplit(' ', 1)
        if len(parts) != 2: continue
        stack, count_str = parts
        count = int(count_str)
        total += count
        frames = stack.split(';')
        leaf = frames[-1]
        leaf_counts[leaf] = leaf_counts.get(leaf, 0) + count

print('Performance Snapshot Summary')
print('=' * 60)
print(f'Description: $DESCRIPTION')
print(f'Commit:      $SHORT_HASH')
print(f'Date:        $(date -Iseconds)')
print(f'Total samples: {total:,}')
print()
print('Top 20 functions by CPU samples:')
print('-' * 60)
sorted_leaves = sorted(leaf_counts.items(), key=lambda x: -x[1])
for i, (leaf, count) in enumerate(sorted_leaves[:20], 1):
    pct = count / total * 100
    print(f'  {i:2d}. {pct:6.2f}%  {leaf}')
print()
print('Category breakdown:')
print('-' * 60)

categories = {}
with open('$SNAPSHOT_DIR/collapsed.txt') as f:
    for line in f:
        line = line.strip()
        if not line: continue
        parts = line.rsplit(' ', 1)
        if len(parts) != 2: continue
        stack, count_str = parts
        count = int(count_str)

        if 'process_chunk' in stack and ('aesni_ctr32' in stack or 'encrypt_chunk' in stack or 'seal_in_place' in stack or 'gcm_gmult' in stack or 'aead_aes_gcm' in stack):
            cat = 'App encryption'
        elif 'jent_' in stack or 'xoshiro' in stack or 'jitter' in stack or 'tree_jitter' in stack:
            cat = 'Jitter entropy (RNG)'
        elif 'rustls' in stack or 'tls_rustls' in stack or 'tokio_rustls' in stack:
            cat = 'TLS'
        elif 'h2::' in stack or 'hyper' in stack:
            cat = 'HTTP/h2'
        elif 'BufferPool' in stack or 'buffer_pool' in stack:
            cat = 'BufferPool'
        elif 'tokio::' in stack:
            cat = 'Tokio runtime'
        elif 'aesni_ctr32' in stack or 'gcm_gmult' in stack or 'aead_aes_gcm' in stack:
            cat = 'AES-GCM (unattributed)'
        else:
            cat = 'Other'
        categories[cat] = categories.get(cat, 0) + count

for cat, count in sorted(categories.items(), key=lambda x: -x[1]):
    pct = count / total * 100
    if pct > 0.05:
        print(f'  {pct:6.2f}%  {cat}')
" > "$SNAPSHOT_DIR/summary.txt"

# --- Remove raw perf.data to save space ---
rm -f "$PERF_DATA"

# --- Done ---
echo ""
echo "=== Snapshot saved ==="
echo "  $SNAPSHOT_DIR/"
echo "    collapsed.txt  - diffable collapsed stacks"
echo "    flamegraph.svg - visual flamegraph"
echo "    summary.txt    - hotspot summary"
echo ""
echo "To compare with another snapshot:"
echo "  ./perf/diff.sh $SNAPSHOT_DIR <other-snapshot-dir>"

cat "$SNAPSHOT_DIR/summary.txt"
