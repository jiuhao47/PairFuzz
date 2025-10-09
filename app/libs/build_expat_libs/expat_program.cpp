// Program wrapper for Expat harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the core implementation which provides expat_xmlparse
#include "expat_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(expat_xmlparse);
  return 0;
}
