// Auto-generated program wrapper for openh264_harness (follow libpng_program.cc
// style). This file defines the DIFFRAY entrypoint and includes the harness
// implementation so the harness functions are available for DIFFRAY_RUN.

#include <stddef.h>
#include <stdint.h>

// Initialize DIFFRAY runtime/macros (project-provided)
DIFFRAY_INIT();

// Include the harness implementation directly so the DIFFRAY macros
// can be used to adapt its compilation, mirroring libpng_program.cc.
#include "openh264_core.cpp"

int main(int argc, char **argv) {
  // Use the harness parsing entry as the target for DIFFRAY_RUN.
  DIFFRAY_RUN(openh264_parse);
  return 0;
}
