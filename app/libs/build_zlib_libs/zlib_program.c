// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides zlib_parse
#include "zlib_core.c"

int main(int argc, char **argv) {
  DIFFRAY_RUN(zlib_parse);
  return 0;
}
