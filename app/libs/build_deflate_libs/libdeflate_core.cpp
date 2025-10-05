// Core implementation moved from libdeflate_harness.cpp
#include <assert.h>
#include <fcntl.h>
#include <libdeflate.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
// C++ file, IO support
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

unsigned char memory_in[10000];
unsigned char memory_out[3000000];

extern "C" int libdeflate_parse(const uint8_t *data, size_t sz) {

  if (sz <= 1)
    return 100;
  struct libdeflate_decompressor *d;
  size_t approximate_sz = sz * 50;

  unsigned char *in;
  unsigned char *out;

  if (sz < 10000) {
    memcpy(memory_in, data, sz);
    in = memory_in;
    out = memory_out;
  } else {
    in = (unsigned char *)calloc(1, sz);
    out = (unsigned char *)calloc(1, approximate_sz);
    memcpy(in, data, sz);
  }
  d = libdeflate_alloc_decompressor();
  size_t out_size = 0;

  enum libdeflate_result res =
      libdeflate_zlib_decompress(d, in, sz, out, approximate_sz, &out_size);
  if (sz >= 10000) {
    free(in);
    free(out);
  }

  int ret = res;
  if (ret != 1)
    ret = 0;

  libdeflate_free_decompressor(d);

  if (ret == 1) {
    printf("decompressed:%d, outsize:%zu, approximate_sz:%zu\n", res, out_size,
           approximate_sz);
  }

  return ret;
}
