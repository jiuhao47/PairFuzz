#!/usr/bin/env bash
# find_mismatch_results.sh
# Usage: find_mismatch_results.sh <directory> [openssl_bin] [wolfssl_bin]
# Default binaries: ./openssl and ./wolfssl (relative to current working dir)

set -u

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <directory> [openssl_bin] [wolfssl_bin]"
  exit 2
fi

#!/usr/bin/env bash
# find_mismatch_results.sh
# Usage: find_mismatch_results.sh <directory> [openssl_bin] [wolfssl_bin]
# Default binaries: ./openssl and ./wolfssl (relative to current working dir)

set -u

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <directory> [openssl_bin] [wolfssl_bin]"
  exit 2
fi

DIR="$1"
OPENSSL_BIN="${2:-./openssl}"
WOLFSSL_BIN="${3:-./wolfssl}"

if [[ ! -d "$DIR" ]]; then
  echo "Error: directory '$DIR' does not exist or is not a directory"
  exit 3
fi

if [[ ! -x "${OPENSSL_BIN}" ]]; then
  echo "Warning: openssl binary '${OPENSSL_BIN}' not executable or not found"
fi
if [[ ! -x "${WOLFSSL_BIN}" ]]; then
  echo "Warning: wolfssl binary '${WOLFSSL_BIN}' not executable or not found"
fi

matches=()
count_total=0
count_mismatch=0

# Helper: run a binary with the file, capture output and extract first integer result
run_and_extract() {
  local bin="$1"
  local file="$2"
  local out
  if ! out="$("$bin" "$file" 2>&1)"; then
    true
  fi
  # extract first integer using awk (portable)
  local num
  num=$(printf '%s' "$out" | awk '/-?[0-9]+/ { match($0, /-?[0-9]+/); print substr($0, RSTART, RLENGTH); exit }')
  # print num on first line, then the full output
  printf '%s\n%s\n' "$num" "$out"
}

# Iterate non-recursively over regular files (sorted)
while IFS= read -r -d '' path; do
  count_total=$((count_total+1))

  openssl_capture=$(run_and_extract "$OPENSSL_BIN" "$path")
  openssl_num=$(printf '%s' "$openssl_capture" | head -n1)
  openssl_out=$(printf '%s' "$openssl_capture" | sed -n '2,100p')

  wolfssl_capture=$(run_and_extract "$WOLFSSL_BIN" "$path")
  wolfssl_num=$(printf '%s' "$wolfssl_capture" | head -n1)
  wolfssl_out=$(printf '%s' "$wolfssl_capture" | sed -n '2,100p')

  # consider only when both numbers are non-empty (numeric)
  if [[ -n "$openssl_num" && -n "$wolfssl_num" && "$openssl_num" =~ ^-?[0-9]+$ && "$wolfssl_num" =~ ^-?[0-9]+$ ]]; then
    if { [[ "$openssl_num" -eq 0 && "$wolfssl_num" -ne 0 ]] || [[ "$openssl_num" -ne 0 && "$wolfssl_num" -eq 0 ]]; } then
      count_mismatch=$((count_mismatch+1))
      matches+=("$path|$openssl_num|$wolfssl_num")
      echo "MISMATCH: $path"
      echo "  openssl: $openssl_num"
      echo "  wolfssl : $wolfssl_num"
      echo "  openssl output (truncated):"
      printf '    %s\n' "$(printf '%s' "$openssl_out" | head -n5 | sed 's/^/    /')"
      echo "  wolfssl output (truncated):"
      printf '    %s\n' "$(printf '%s' "$wolfssl_out" | head -n5 | sed 's/^/    /')"
      echo
    fi
  else
    # If parse failed, optionally report
    if [[ -z "$openssl_num" || -z "$wolfssl_num" ]]; then
      echo "PARSE-FAIL: $path (couldn't extract integer from one or both outputs)"
      echo "  openssl raw output (first 5 lines):"
      printf '    %s\n' "$(printf '%s' "$openssl_out" | head -n5 | sed 's/^/    /')"
      echo "  wolfssl raw output (first 5 lines):"
      printf '    %s\n' "$(printf '%s' "$wolfssl_out" | head -n5 | sed 's/^/    /')"
      echo
    fi
  fi

done < <(find "$DIR" -maxdepth 1 -type f -print0 | sort -z)

# Summary
echo "Checked files: $count_total"
if [[ $count_mismatch -gt 0 ]]; then
  echo "Mismatches (one side 0, other non-zero): $count_mismatch"
  # print CSV-like list
  printf '%s\n' "${matches[@]}" | sed 's/|/,/g'
else
  echo "No one-zero/other-nonzero mismatches found."
fi

exit 0
