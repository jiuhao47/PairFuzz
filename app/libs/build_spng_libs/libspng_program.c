// Program wrapper that includes the core implementation and the harness
// so the translation unit contains the implementation without modifying
// the Makefile.
#include "libspng_core.c"

/* select_rv_shm_env is provided by app/libs/rv_shm/rv_shm.c */
DIFFRAY_INIT();

int main(int argc, char **argv) {
  DIFFRAY_RUN(libspng_parse);
  return 0;
}