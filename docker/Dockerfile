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
COPY docker/sshd-start.sh /usr/sbin/sshd-start.sh
RUN chmod +x /usr/sbin/sshd-start.sh

# Create a non-root user to run builds
ARG USER=dev
ARG UID=1000
ARG DEV_PUBKEY=""
RUN useradd -m -u ${UID} ${USER} || true 

# Set a per-user cargo home so non-root builds don't try to write to /usr/local/cargo
ENV CARGO_HOME=/home/${USER}/.cargo
# Add PATH patch: entries present in the runtime PATH but not in the minimal base PATH
# Second PATH differs from base by adding: /home/dev/.cargo/bin, /usr/local/cargo/bin, /usr/local/sbin, /usr/sbin, /sbin
ENV PATH_PATCH=/home/${USER}/.cargo/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/sbin:/sbin
ENV PATH=${PATH_PATCH}:${CARGO_HOME}/bin:$PATH
RUN mkdir -p /home/${USER}/.cargo && chown -R ${USER}:${USER} /home/${USER}/.cargo

USER ${USER}

WORKDIR /home/${USER}/workspace

# If a developer public key was provided at build time, install it for `dev` user
USER root
RUN if [ -n "${DEV_PUBKEY}" ]; then \
    mkdir -p /home/${USER}/.ssh && \
    echo "${DEV_PUBKEY}" > /home/${USER}/.ssh/authorized_keys && \
    chown -R ${USER}:${USER} /home/${USER}/.ssh && \
    chmod 700 /home/${USER}/.ssh && chmod 600 /home/${USER}/.ssh/authorized_keys; \
    fi
USER ${USER}

# Default shell when running container interactively
CMD ["bash"]
