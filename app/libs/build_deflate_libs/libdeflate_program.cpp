// Program wrapper for libdeflate harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the core implementation which provides libdeflate_parse
#include "libdeflate_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(libdeflate_parse);
  return 0;
}
