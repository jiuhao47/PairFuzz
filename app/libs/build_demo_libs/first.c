#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  //   printf("Hello from first.c: diffRayTestOneInput_1\n");
  int ret = 0;
  if (data[0] == 'P') {
    printf("data[0]=%c\n, Aborting", data[0]);
    abort();
  } else if (size > 64) {
    printf("Size > 64, Aborting");
    abort();
  } else {
    printf("data[0]=%c\n, size=%zu\n", data[0], size);
  }
  return ret;
}