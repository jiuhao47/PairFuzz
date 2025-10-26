#!/usr/bin/env bash
set -euo pipefail

# cleanup_shm.sh — Interactive SysV shared memory cleanup script (English)
# - Lists SysV shared memory segments where nattch == 0 (no attached processes)
# - Prompts for confirmation and deletes those segments with `ipcrm -m` if confirmed
# Usage:
#   chmod +x cleanup_shm.sh
#   ./cleanup_shm.sh

printf "Listing SysV shared memory segments (header + segments with nattch==0):\n\n"

# Print header and any lines where nattch == 0 (robust to different ipcs output formats)
# Detect header by matching the header line containing both 'shmid' and 'nattch'
ipcs -m | awk '
/shmid/ && /nattch/ {
  for (i=1;i<=NF;i++) {
    h=tolower($i)
    if (h=="nattch") nattch_idx=i
    if (h=="shmid") shmid_idx=i
  }
  print
  next
}
(nattch_idx && $nattch_idx==0) { print }
'

echo
read -rp "Type 'yes' to DELETE all listed shmid values (case-sensitive): " confirm
if [[ "$confirm" != "yes" ]]; then
  echo "Abort: not confirmed. No changes made."
  exit 0
fi

# Collect shmid values where nattch == 0
shmids=$(ipcs -m | awk '
/shmid/ && /nattch/ {
  for (i=1;i<=NF;i++) {
    h=tolower($i)
    if (h=="nattch") nattch_idx=i
    if (h=="shmid") shmid_idx=i
  }
  next
}
(nattch_idx && shmid_idx && $nattch_idx==0) { print $shmid_idx }
')

if [[ -z "$shmids" ]]; then
  echo "No shmids found with nattch==0. Nothing to delete."
  exit 0
fi

# Delete each shmid (requires root privileges)
for id in $shmids; do
  echo "Removing shmid=$id ..."
  if ipcrm -m "$id"; then
    echo "Removed $id"
  else
    echo "Failed to remove $id" >&2
  fi
done

echo "Done."
