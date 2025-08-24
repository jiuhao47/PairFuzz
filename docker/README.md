# diffray Docker development image

This folder contains a simple Dockerfile to create a development/execution image for the diffray project.

Features:
- Rust toolchain (from official rust image)
- Common native build dependencies (openssl, zlib, libpng, libxml2, xz, etc.)
- Non-root user `dev` to run builds

How to build the image:

```bash
# from repository root
docker build -t diffray-dev -f docker/Dockerfile .
```

Run an interactive container mounting the repo (recommended for iterative development):

```bash
docker run --rm -it \
  -v "$PWD":/home/dev/workspace \
  -w /home/dev/workspace \
  -p 8080:8080 \
  diffray-dev
```

Run a one-shot build inside the container:

```bash
docker run --rm -it \
  -v "$PWD":/home/dev/workspace \
  -w /home/dev/workspace \
  diffray-dev \
  bash -lc "cargo build --release"
```

Notes and next steps:
- If your project needs extra system libs, install them in the Dockerfile.
- For CI, consider creating a smaller runtime image that copies built artifacts from a builder stage.
