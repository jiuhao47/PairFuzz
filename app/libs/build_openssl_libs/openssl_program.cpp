// Program wrapper for zlib harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the harness implementation which provides verify_cert_mem_openssl
#include "openssl_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(verify_cert_mem_openssl);
  return 0;
}
