// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides verify_wolfssl
#include "wolfssl_core_simple.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(verify_wolfssl);
  return 0;
}
