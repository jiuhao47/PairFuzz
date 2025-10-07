// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides libtar_parse
#include "libtar_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(tar_parse);
  return 0;
}
