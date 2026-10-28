ROOT := ../..
CC := $(ROOT)/target/debug/libafl_forkserver_cc
RUNTIME := $(ROOT)/target/debug/liblibforkserver_libafl_cc.a
OUT := $(ROOT)/target/real-json
CFLAGS := -O2 -g -Wall -Wextra -std=c11
MJSON_DEFS := -DMJSON_ENABLE_PRINT=0 -DMJSON_ENABLE_RPC=0 \
	-DMJSON_ENABLE_BASE64=0 -DMJSON_ENABLE_MERGE=0 \
	-DMJSON_ENABLE_PRETTY=0 -DMJSON_ENABLE_NEXT=0

.PHONY: all clean

all: $(OUT)/cjson_target $(OUT)/mjson_target

$(OUT):
	mkdir -p $@

$(OUT)/cJSON.o: $(ROOT)/third_party/cjson/cJSON.c $(CC) $(RUNTIME) | $(OUT)
	$(CC) $(CFLAGS) -I$(ROOT)/third_party/cjson -c $< -o $@

$(OUT)/mjson.o: $(ROOT)/third_party/mjson/mjson.c $(CC) $(RUNTIME) | $(OUT)
	$(CC) $(CFLAGS) $(MJSON_DEFS) -I$(ROOT)/third_party/mjson -c $< -o $@

$(OUT)/cjson_target.o: cjson_target.c target_io.h $(CC) $(RUNTIME) | $(OUT)
	$(CC) $(CFLAGS) -I$(ROOT)/third_party/cjson -c $< -o $@

$(OUT)/mjson_target.o: mjson_target.c target_io.h $(CC) $(RUNTIME) | $(OUT)
	$(CC) $(CFLAGS) $(MJSON_DEFS) -I$(ROOT)/third_party/mjson -c $< -o $@

$(OUT)/cjson_target: $(OUT)/cjson_target.o $(OUT)/cJSON.o
	$(CC) $^ -o $@

$(OUT)/mjson_target: $(OUT)/mjson_target.o $(OUT)/mjson.o
	$(CC) $^ -lm -o $@

clean:
	rm -f $(OUT)/cJSON.o $(OUT)/mjson.o \
		$(OUT)/cjson_target.o $(OUT)/mjson_target.o \
		$(OUT)/cjson_target $(OUT)/mjson_target
