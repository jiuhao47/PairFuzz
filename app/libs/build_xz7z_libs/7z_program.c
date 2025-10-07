// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides cli_scanxz
#include "7z_core.c"

int main(int argc, char **argv) {
  DIFFRAY_RUN(cli_scanxz);
  return 0;
}
