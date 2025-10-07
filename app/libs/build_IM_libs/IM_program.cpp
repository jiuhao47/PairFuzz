// Program wrapper for ImageMagick harness.
// Provides a main that runs the core parse function via DIFFRAY macros.
#include <stddef.h>
#include <stdint.h>

DIFFRAY_INIT();

// include the core implementation which provides imagemagick_parse
#include "IM_core.cpp"

int main(int argc, char **argv) {
  DIFFRAY_RUN(imagemagick_parse);
  return 0;
}
