// Program wrapper for GraphicsMagick harness.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the core implementation which provides GM_parse
#include "GM_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(GM_parse);
  return 0;
}
