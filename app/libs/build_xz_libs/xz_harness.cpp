#include <assert.h>
#include <stdint.h>
#include <unistd.h>
#include <stdbool.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <lzma.h>

#define OUTPUT_BUF_SIZE 65536*16

struct GlobalState {
    GlobalState()
    : outbuf(InitOutBuf()) { }

    ~GlobalState() {
        free(outbuf);
    }

    unsigned char *InitOutBuf() {
        unsigned char *tmpbuf = (unsigned char *)malloc(OUTPUT_BUF_SIZE);
        assert(tmpbuf != NULL);
        return tmpbuf;
    }

    unsigned char *const outbuf;
};

static int xz_decompress(const uint8_t *data, size_t size)
{
    static GlobalState g_state;
	int rc;
	int nread = 0;
	lzma_action action = LZMA_RUN;

	lzma_ret ret;
	lzma_stream strm = LZMA_STREAM_INIT;

	if (lzma_stream_decoder(&strm, UINT64_MAX, LZMA_CONCATENATED) != LZMA_OK){
		printf("init xz decoder error:%d\n", rc);
		return -1;
	}

	strm.avail_in = size;
	strm.next_in = data;
	strm.next_out = g_state.outbuf;
	strm.avail_out = OUTPUT_BUF_SIZE;

	while (true) {
		if ((ret = lzma_code(&strm, LZMA_FINISH)) != LZMA_OK) break;
		if (strm.avail_out == 0) {
			// outbuf became full. We don't care about the
			// uncompressed data there, so we simply reuse
			// the outbuf and overwrite the old data.
			strm.next_out = g_state.outbuf;
			strm.avail_out = OUTPUT_BUF_SIZE;
		}

	}

	lzma_end(&strm);

	rc = ((ret == LZMA_OK) ||
          (ret == LZMA_STREAM_END)) ? 0 : ret;
	
	if (rc != 0) {
		printf("decode xz error:%d\n", rc);
	}
	return rc;
		
}


extern "C" int xz_parse(const uint8_t* data, size_t size)
{
	// check file header 
	if (size<6 || data[0]!=0xfd || data[1]!=0x37 || data[2] != 0x7a || data[3] != 0x58 || data[4]!=0x5a || data[5]!=0x00)
        // return -1;
		return 1;

    int ret = xz_decompress(data,size);
	
    return (ret==0)?0:1;
}
extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return xz_parse(data, size);
}