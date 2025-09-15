#include <assert.h>
#include <libdeflate.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/stat.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdint.h>
// C++ file, IO support
#include <fstream>
#include <iostream>
#include <string>
#include <vector>
#include <string.h>
unsigned char memory_in[10000];
unsigned char memory_out[3000000];

extern "C" int libdeflate_parse(const uint8_t *data, size_t sz) {
	
	if (sz<=1) return 100;
	struct libdeflate_decompressor *d;
    size_t approximate_sz = sz * 50;

	unsigned char *in;
	unsigned char *out;

	if (sz < 10000) {
		memcpy(memory_in, data, sz);
		in = memory_in;
		out = memory_out;
	}else {
		in = (unsigned char *) calloc(1, sz);
		out = (unsigned char *) calloc(1, approximate_sz);
		memcpy(in, data, sz);
	}
	d = libdeflate_alloc_decompressor();
	size_t out_size = 0 ;

	// sz => in_nbytes, approximate_sz => out_nbytes_avail
	// out_size => actual_out_nbytes_ret
	enum libdeflate_result res = libdeflate_zlib_decompress(d, in, sz, out, approximate_sz, &out_size);
	/* free memory */
	if (sz>=10000) {
		free(in);
		free(out);
	}
	
	int ret = res;
	if (ret != 1) // only res=1 means invalid
		ret = 0;
		
	libdeflate_free_decompressor(d);

    if (ret == 1) {
        printf("decompressed:%d, outsize:%zu, approximate_sz:%zu\n", res, out_size, approximate_sz);
    }

	return ret;
}

int main(int argc,char **argv) {
	if (argc !=2) {
		printf("usage: %s [filename]\n", argv[0]);
		return 0;
	}

	std::ifstream in(argv[1]);
	in.seekg(0, in.end);
	size_t length = in.tellg();
	in.seekg (0, in.beg);
	std::vector<char> bytes(length);
	in.read(bytes.data(), bytes.size());
	int ret = libdeflate_parse(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());
	if (ret != 0) {
		printf("parse error!\n");
	}

	return 0;
}
extern "C" int diffRayTestOneInput_2(const uint8_t *data, size_t size) {
  return libdeflate_parse(data, size);
}

