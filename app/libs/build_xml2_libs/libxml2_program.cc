// Program wrapper for libxml2 harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the core implementation which provides xml2_xmlparse
#include "libxml2_core.cc"

int main(int argc, char **argv) {
  DIFFRAY_RUN(xml2_xmlparse);
  return 0;
}
