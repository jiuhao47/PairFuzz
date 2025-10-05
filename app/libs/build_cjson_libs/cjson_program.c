// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides cjson_parse
#include "cjson_core.c"

int main(int argc, char **argv) {
  DIFFRAY_RUN(cjson_parse);
  return 0;
}
