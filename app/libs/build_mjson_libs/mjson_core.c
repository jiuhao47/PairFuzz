#include "mjson.h"
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

int mjson_parse(const unsigned char *data, size_t size) {
  if (size == 0) {
    return 100;
  }

  char *buf = (char *)malloc(size + 1);
  buf[size] = 0;
  memcpy(buf, data, size);
  const char *p;
  int n = 0;
  int len = (int)strlen(buf);
  int the_type = mjson_find(buf, len, "$", &p, &n);
  int ret = 1;
  if (the_type == 123) {
    ret = 0;
  }
  free(buf);
  return ret;
}
