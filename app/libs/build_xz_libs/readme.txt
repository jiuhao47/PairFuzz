### version
oldest: 98f3cac1a
newest: 15cf3f04


### patch logic

new version中有patch逻辑，enable一下好就好,FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION


### copy command

find ./ -name *.so* -exec cp -P {} /home/fdl/diff-fuzz/examples/builds/libs/xz_newest/lib \;

### build command

autogen.sh

../configure --disable-ifunc CC="clang" CXX="clang++" CFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC" CXXFLAGS="-g -fsanitize=address,fuzzer-no-link -fsanitize-coverage=edge,indirect-calls -fPIC"

make 

make install DESTDIR=/home/fdl/diff-fuzz/examples/src/libs/tmp/

cp -r /home/fdl/diff-fuzz/examples/src/libs/tmp/usr/local/include /home/fdl/diff-fuzz/examples/builds/libs/xz_newest/

### From newest patch
1. block_header_decoder.c
	// Verify CRC32
	if (lzma_crc32(in, in_size, 0) != read32le(in + in_size)) {
    // #ifndef FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION
    // 		return LZMA_DATA_ERROR;
    // #endif
	}
2. index_decoder.c

    case SEQ_CRC32:
            do {
                if (*in_pos == in_size)
                    return LZMA_OK;

                if (((coder->crc32 >> (coder->pos * 8)) & 0xFF)
                        != in[(*in_pos)++]) {
                // #ifndef FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION
                // 				return LZMA_DATA_ERROR;
                // #endif
                }
3. stream_flags_decoder.c (function: lzma_stream_header_decode)

    const uint32_t crc = lzma_crc32(in + sizeof(lzma_header_magic),
                LZMA_STREAM_FLAGS_SIZE, 0);
        if (crc != read32le(in + sizeof(lzma_header_magic)
                + LZMA_STREAM_FLAGS_SIZE)) {
            // #ifndef FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION
            // 		return LZMA_DATA_ERROR;
            // #endif
        }
4. stream_flags_decoder.c (function: lzma_stream_footer_decode)

    const uint32_t crc = lzma_crc32(in + sizeof(uint32_t),
			sizeof(uint32_t) + LZMA_STREAM_FLAGS_SIZE, 0);
	if (crc != read32le(in)) {
        // #ifndef FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION
        // 		return LZMA_DATA_ERROR;
        // #endif
	}

5. index_hash.c
if (((index_hash->crc32 >> (index_hash->pos * 8))
					& 0xFF) != in[(*in_pos)++]) {
#ifndef FUZZING_BUILD_MODE_UNSAFE_FOR_PRODUCTION
				// return LZMA_DATA_ERROR;
#endif
			}

### For oldest patch
1. block_header_decoder.c

the path: ./src/liblzma/common/block_header_decoder.c

if (lzma_crc32(in, in_size, 0) != integer_read_32(in + in_size)){
		// return LZMA_DATA_ERROR;
	}

2. index_decoder.c
the path: ./src/liblzma/common/index_decoder.c  [?]

if (((coder->crc32 >> (coder->pos * 8)) & 0xFF)
					!= in[(*in_pos)++]) {
				// return LZMA_DATA_ERROR;
					}

3.stream_flags_decoder.c
the path: ./src/liblzma/common/stream_flags_decoder.c

if (crc != integer_read_32(in + sizeof(lzma_header_magic)
			+ LZMA_STREAM_FLAGS_SIZE))
		return LZMA_DATA_ERROR;

4.stream_flags_decoder.c
if (crc != integer_read_32(in))
		return LZMA_DATA_ERROR;

5. index_hash.c
the path: ./src/liblzma/common/index_hash.c

```
git apply patched_old.patch
```
