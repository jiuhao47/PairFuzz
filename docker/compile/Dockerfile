# Development Dockerfile for diffray
# Provides Rust toolchain and common native build dependencies
FROM rust:1.87-slim

LABEL maintainer="diffray <dev@local>"

# Install system dependencies commonly needed to build the repository
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    m4 \
    lsb-release \
    build-essential \
    cmake \
    pkg-config \
    libssl-dev \
    zlib1g-dev \
    libpng-dev \
    libxml2-dev \
    liblzma-dev \
    libbz2-dev \
    libjpeg-dev \
    git \
    vim \
    htop \
    tmux \
    openssh-server \
    libtool \
    automake \
    mercurial \
    python3.11 \
    python3-dev \
    libpython3.11-dev \
    nasm \
    && rm -rf /var/lib/apt/lists/*

## Install LLVM 18 (clang-18, lld-18, libc++ etc.) from apt.llvm.org
# We detect the distro codename and add the LLVM APT repository, then install packages.
RUN set -eux; \
    DISTRO_CODENAME="$(lsb_release -cs 2>/dev/null || echo bookworm)"; \
    # fetch LLVM GPG key and store as a keyring, then add apt source with signed-by
    curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key | gpg --dearmor -o /usr/share/keyrings/llvm-archive-keyring.gpg; \
    echo "deb [signed-by=/usr/share/keyrings/llvm-archive-keyring.gpg] http://apt.llvm.org/${DISTRO_CODENAME}/ llvm-toolchain-${DISTRO_CODENAME}-18 main" > /etc/apt/sources.list.d/llvm18.list; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
    clang-18 \
    lld-18 \
    llvm-18-dev \
    libclang-18-dev \
    libc++-18-dev \
    libc++abi-18-dev \
    clang-format-18 \
    clang-tidy-18 \
    ; \
    rm -rf /var/lib/apt/lists/*;

RUN set -eux; \
    # Try to install ASan runtime artifacts (compiler-rt / libclang_rt.asan). Package names
    # vary by distro and apt source, so we try several likely candidates and continue on error.
    apt-get update; \
    apt-get install -y --no-install-recommends compiler-rt-18 || true; \
    apt-get install -y --no-install-recommends llvm-18-runtime || true; \
    apt-get install -y --no-install-recommends libclang-18-dev libclang-rt-18-dev || true; \
    apt-get install -y --no-install-recommends libasan6 libasan7 libasan8 || true; \
    rm -rf /var/lib/apt/lists/*; \
    echo '=== Verify ASan runtimes ==='; \
    if [ -d /usr/lib/llvm-18/lib/clang/18/lib/linux ]; then \
    echo 'Files in /usr/lib/llvm-18/lib/clang/18/lib/linux:'; \
    ls -la /usr/lib/llvm-18/lib/clang/18/lib/linux || true; \
    else \
    echo '/usr/lib/llvm-18/lib/clang/18/lib/linux not present; searching for libclang_rt.asan*...'; \
    find /usr -name 'libclang_rt.asan*' -print || echo 'No libclang_rt.asan* found'; \
    fi; \
    echo '=== Verify dynamic libasan *.so (if any) ==='; \
    ldconfig -p | grep libasan || true

# Make clang-18 / clang++-18 the default clang/clang++ in the image
RUN update-alternatives --install /usr/bin/clang clang /usr/bin/clang-18 100 \
    && update-alternatives --install /usr/bin/clang++ clang++ /usr/bin/clang++-18 100 || true

RUN cargo install --locked just --version 1.42.4

# SSH configuration: install openssh-server above; default SSH port can be overridden
ENV SSH_PORT=2222

# Provide a small helper to start sshd with a configurable port. We keep the image
# default user as non-root (dev) for building, but when running the container in
# SSH mode the deploy script runs the container as root so the sshd process can bind.
RUN mkdir -p /var/run/sshd /etc/ssh && ssh-keygen -A || true
COPY docker/common/sshd-start.sh /usr/sbin/sshd-start.sh
RUN chmod +x /usr/sbin/sshd-start.sh

## Run as root (no non-root user). Files created by root map to host root unless adjusted.
## Use the container-start.sh helper (added below) to adjust ownership/permissions of
## mounted folders at runtime so host users can access files created by root.

## Use system-wide cargo and rustup locations so ssh sessions see the same
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV RUST_VERSION=1.87.0
# Add PATH patch: include cargo and rustup bins and common sbin dirs
ENV PATH_PATCH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/sbin:/sbin
ENV PATH=${PATH_PATCH}:$PATH
RUN mkdir -p /usr/local/cargo /usr/local/rustup && chown -R root:root /usr/local/cargo /usr/local/rustup

# Ensure login and interactive shells receive CARGO_HOME, RUSTUP_HOME and PATH
RUN printf '%s\n' \
    'export CARGO_HOME=/usr/local/cargo' \
    'export RUSTUP_HOME=/usr/local/rustup' \
    'export RUST_VERSION=1.87.0' \
    'export PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/sbin:/sbin:$PATH' \
    >> /etc/profile \
    && printf '%s\n' \
    'export CARGO_HOME=/usr/local/cargo' \
    'export RUSTUP_HOME=/usr/local/rustup' \
    'export RUST_VERSION=1.87.0' \
    'export PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/sbin:/sbin:$PATH' \
    >> /root/.bashrc

WORKDIR /workspace

# Copy the container start helper and make it executable. The script will run any
# provided command (default bash) after optionally adjusting permissions.
COPY docker/common/container-start.sh /usr/sbin/container-start.sh
RUN chmod +x /usr/sbin/container-start.sh

# If a developer public key was provided at build time, install it for root
ARG DEV_PUBKEY=""
RUN if [ -n "${DEV_PUBKEY}" ]; then \
    mkdir -p /root/.ssh && \
    echo "${DEV_PUBKEY}" > /root/.ssh/authorized_keys && \
    chown -R root:root /root/.ssh && \
    chmod 700 /root/.ssh && chmod 600 /root/.ssh/authorized_keys; \
    fi

# Default entrypoint uses the permission helper. To adjust permissions for a
# mounted workspace so host users can access files, set HOST_UID and HOST_GID
# environment variables when running the container, or set MAKE_WORLD_WRITABLE=1
# to force permissive mode.
ENTRYPOINT ["/usr/sbin/container-start.sh"]
CMD ["bash"]
