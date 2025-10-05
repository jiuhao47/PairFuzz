// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides mjson_parse
#include "mjson_core.c"

int main(int argc, char **argv) {
  DIFFRAY_RUN(mjson_parse);
  return 0;
}
