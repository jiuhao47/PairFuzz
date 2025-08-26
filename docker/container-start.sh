#!/bin/sh
set -e

# Helper to adjust ownership/permissions of mounted directories so host users
# can access files created by root inside the container.
#
# Usage (via docker run):
#  - set HOST_UID and HOST_GID to the host user/group that should own files
#  - or set MAKE_WORLD_WRITABLE=1 to chmod -R a+rwX the target directory
#
# The script looks for the first writable mount under /workspace and adjusts it.

HOST_UID=${HOST_UID:-}
HOST_GID=${HOST_GID:-}
MAKE_WORLD_WRITABLE=${MAKE_WORLD_WRITABLE:-0}

adjust_path="/workspace"

if [ "$MAKE_WORLD_WRITABLE" = "1" ]; then
    echo "Making ${adjust_path} world-writable (a+rwX)"
    chmod -R a+rwX "${adjust_path}" || true
elif [ -n "${HOST_UID}" ] || [ -n "${HOST_GID}" ]; then
    echo "Adjusting ownership of ${adjust_path} to ${HOST_UID:-root}:${HOST_GID:-root}"
    chown -R ${HOST_UID:-0}:${HOST_GID:-0} "${adjust_path}" || true
else
    echo "No HOST_UID/HOST_GID and MAKE_WORLD_WRITABLE not set; leaving permissions as-is"
fi

# If SSH is expected to run in the container, run sshd in foreground so it
# becomes the container's main process (prevents immediate exit/restart loop).
if [ "${START_SSHD}" = "1" ]; then
    echo "Starting sshd via sshd-start.sh (foreground)"
    exec /usr/sbin/sshd-start.sh
fi

# Execute provided command (default bash)
if [ $# -gt 0 ]; then
    exec "$@"
else
    exec bash
fi
