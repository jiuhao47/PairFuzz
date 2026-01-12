#!/usr/bin/env bash
set -euo pipefail

# Build and install the diffray AFL++ interface harness.
#
# Output:
#   app/bins/.bin/diffray/diffray_program

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT_DIR=$(cd "${SCRIPT_DIR}/../../.." && pwd) # app/

CXX_WRAPPER="${ROOT_DIR}/compiler/target/release/libafl_forkserver_cxx"

if [ ! -x "${CXX_WRAPPER}" ]; then
  echo "Error: required wrapper '${CXX_WRAPPER}' not found or not executable" >&2
  exit 1
fi

BUILD_DIR=$(mktemp -d "${SCRIPT_DIR}/temp_build.XXXX")
trap 'rm -rf "${BUILD_DIR}"' EXIT

CXXFLAGS=(
  -std=c++17
  -O2
  -g
  -fno-omit-frame-pointer
  -fsanitize=address
  -I"${SCRIPT_DIR}/include"
)

LDFLAGS=(
  -fsanitize=address
  -ldl
)

# Compile
objs=()
while IFS= read -r -d '' f; do
  obj="${BUILD_DIR}/$(basename "${f}" .cpp).o"
  "${CXX_WRAPPER}" "${CXXFLAGS[@]}" -c "${f}" -o "${obj}"
  objs+=("${obj}")
done < <(find "${SCRIPT_DIR}/src" -maxdepth 1 -name '*.cpp' -print0)

# Link
"${CXX_WRAPPER}" "${objs[@]}" "${LDFLAGS[@]}" -o "${BUILD_DIR}/diffray_program"

# Install
INSTALL_DIR="${ROOT_DIR}/bins/.bin/diffray"
mkdir -p "${INSTALL_DIR}"
install -m 0755 "${BUILD_DIR}/diffray_program" "${INSTALL_DIR}/diffray_program"

echo "Done: ${INSTALL_DIR}/diffray_program"
