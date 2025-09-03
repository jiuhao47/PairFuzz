#include "zlib.h"
#include <stdio.h>
#include <assert.h>
#include <stdint.h>
#include <unistd.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
// #include "util.h"

int FDL_DEBUG_TMP = 0;
static const size_t dataLen = 10000;
unsigned char uncompr[10000];
unsigned char memory[10000];

int wrap_inf(unsigned char *data, int len)
{
    int ret;
    int err;
    z_stream strm;
    /* allocate inflate state */
    strm.zalloc = Z_NULL;
    strm.zfree = Z_NULL;
    strm.opaque = (void *)0;
    strm.avail_in = (unsigned int) len;  /* number of bytes available at next_in */
    strm.next_in = data;/* next input byte */
    
    ret = inflateInit(&strm);
    if (ret != Z_OK) {
        if (FDL_DEBUG_TMP) printf("[FDL] error after inflateInit @ %s, %d\n", __FILE__, __LINE__);
        return ret;
    }

    // uncompr = (uint8_t *)calloc(1, dataLen);
    // strm.next_out = uncompr;
    // strm.avail_out = (unsigned int) dataLen;

    /* decompress until deflate stream ends or error happens*/
    for (;;) {
        strm.next_out = uncompr;
        strm.avail_out = (unsigned int) dataLen;
        err = inflate(&strm, Z_NO_FLUSH);
        if (err == Z_STREAM_END) {
            /* clean up and return */
            (void)inflateEnd(&strm);
            if (FDL_DEBUG_TMP) printf("parse completed with Z_STREAM_END \n");
            // free(uncompr);
            return 0;
        }
        if (err != Z_OK) {
            /* clean up and return */
            (void)inflateEnd(&strm);
            if (FDL_DEBUG_TMP) printf("parse error: %d \n", err);
            
            // free(uncompr)
            return err;
        }

    }
    // will never go here, but we leave a tag here
    if (FDL_DEBUG_TMP) printf("unexpected error, it should never go here\n");
    
    return 100;
}


extern int zlib_parse(const uint8_t *data, size_t sz){

    if (sz<=1) return 200;
    int ret;

    if (sz<=10000) {
        memcpy(memory, data, sz);
        ret = wrap_inf(memory, sz); 
    }else {
        unsigned char *mem = (unsigned char *)calloc(1,sz);
        memcpy(mem,data,sz);
        ret = wrap_inf(mem, sz);
        free(mem);
    }

    // return ret;
    return (ret==0)?0:1;

}

extern int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return zlib_parse(data, size);
}

