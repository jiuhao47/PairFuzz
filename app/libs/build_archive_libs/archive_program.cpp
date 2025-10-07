// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides archive_parse
#include "archive_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(archive_parse);
  return 0;
}
