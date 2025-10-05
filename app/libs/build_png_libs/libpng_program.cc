
// libpng_read_fuzzer.cc
// Copyright 2017-2018 Glenn Randers-Pehrson
// Copyright 2015 The Chromium Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that may
// be found in the LICENSE file https://cs.chromium.org/chromium/src/LICENSE

// The modifications in 2017 by Glenn Randers-Pehrson include
// 1. addition of a PNG_CLEANUP macro,
// 2. setting the option to ignore ADLER32 checksums,
// 3. adding "#include <string.h>" which is needed on some platforms
//    to provide memcpy().
// 4. adding read_end_info() and creating an end_info structure.
// 5. adding calls to png_set_*() transforms commonly used by browsers.
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

// Program wrapper: include lightweight harness and run via DIFFRAY macros.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness wrapper implementation which forwards to libpng_core
#include "libpng_core.cc"

int main(int argc, char **argv) {
  DIFFRAY_RUN(libpng_parse);
  return 0;
}