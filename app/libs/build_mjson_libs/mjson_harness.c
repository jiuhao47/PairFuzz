
#include <stdint.h>
#include <stddef.h>
#include <float.h>  // For DBL_EPSILON
#include <math.h>
#include <stdio.h>  // For printf
#include "mjson.h"


extern int mjson_parse(const unsigned char *data, size_t size) {
  
    if (size==0){
        return 100;
    }

    char *buf = (char *)malloc(size+1);
    buf[size] = 0;
    memcpy(buf, data, size);
    const char *p;
    int n=0;
    int len = (int)strlen(buf);
    int the_type = mjson_find(buf, len, "$", &p, &n);

    /* means there is an error */
    int ret = 1; 

    if (the_type==123) { 
        ret = 0;
    } 
    free(buf);

    return ret;
}
extern int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return mjson_parse(data, size);
}