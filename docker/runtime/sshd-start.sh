#!/bin/sh
set -e
# Configurable via env: SSH_PORT (default 2222)
: "${SSH_PORT:=2222}"
if [ ! -f /etc/ssh/ssh_host_rsa_key ]; then
    ssh-keygen -A
fi
if grep -q '^Port ' /etc/ssh/sshd_config; then
    sed -i "s/^Port .*/Port ${SSH_PORT}/" /etc/ssh/sshd_config
else
    echo "Port ${SSH_PORT}" >> /etc/ssh/sshd_config
fi
# Always disable password authentication to enforce key-based auth only
sed -i "s/^#\?PasswordAuthentication .*/PasswordAuthentication no/" /etc/ssh/sshd_config || echo "PasswordAuthentication no" >> /etc/ssh/sshd_config
# Allow root login with public key only (disable password auth already enforced above)
sed -i 's/^#\?PermitRootLogin.*/PermitRootLogin prohibit-password/' /etc/ssh/sshd_config || true

# Ensure authorized_keys permissions correct
if [ -d /root/.ssh ]; then
    chown -R root:root /root/.ssh || true
    chmod 700 /root/.ssh || true
    chmod 600 /root/.ssh/authorized_keys 2>/dev/null || true
fi

exec /usr/sbin/sshd -D -e
