ROOT := $(abspath ../..)
OUT := $(ROOT)/target/contest
WRAPPER := $(ROOT)/target/debug/libafl_forkserver_cc
RUNTIME := $(ROOT)/target/debug/liblibforkserver_libafl_cc.a

.PHONY: all native
all: $(OUT)/demo_ref $(OUT)/demo_alt

$(OUT):
	mkdir -p "$@"

$(OUT)/demo_ref: target.c $(WRAPPER) $(RUNTIME) | $(OUT)
	"$(WRAPPER)" -O1 -g -DACCEPT_HEX=1 "$<" -o "$@" -ldl -lpthread -lm

$(OUT)/demo_alt: target.c $(WRAPPER) $(RUNTIME) | $(OUT)
	"$(WRAPPER)" -O1 -g -DACCEPT_HEX=0 "$<" -o "$@" -ldl -lpthread -lm

native: | $(OUT)
	$(CC) -O1 -g -Wall -Wextra -DACCEPT_HEX=1 target.c -o "$(OUT)/native_ref"
	$(CC) -O1 -g -Wall -Wextra -DACCEPT_HEX=0 target.c -o "$(OUT)/native_alt"
