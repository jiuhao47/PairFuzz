// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides xz_parse
#include "xz_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(xz_parse);
  return 0;
}
