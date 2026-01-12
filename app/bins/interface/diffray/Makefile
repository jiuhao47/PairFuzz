# diffray AFL++ interface harness Makefile
#
# Targets:
#   make            - build ./build/diffray_program
#   make install    - install to ../../bins/.bin/diffray/diffray_program
#   make clean      - remove build artifacts
#
# Notes:
# - Uses the repo compiler wrapper by default (libafl_forkserver_cxx).
# - Mirrors compile.sh flags (ASAN + dlopen).

SHELL := /usr/bin/env bash

SCRIPT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
APP_DIR := $(abspath $(SCRIPT_DIR)/../../..)

# Use the repo compiler wrapper by default.
# GNU make sets CXX=g++ by default; only override when CXX is not explicitly set by the user.
ifeq ($(origin CXX),default)
CXX := $(APP_DIR)/compiler/target/release/libafl_forkserver_cxx
endif
ifeq ($(origin CXX),undefined)
CXX := $(APP_DIR)/compiler/target/release/libafl_forkserver_cxx
endif

BUILD_DIR ?= $(SCRIPT_DIR)/build
TARGET := $(BUILD_DIR)/diffray_program

INSTALL_DIR := $(APP_DIR)/bins/.bin/diffray
INSTALL_TARGET := $(INSTALL_DIR)/diffray_program

SRCS := $(wildcard $(SCRIPT_DIR)/src/*.cpp)
OBJS := $(patsubst $(SCRIPT_DIR)/src/%.cpp,$(BUILD_DIR)/%.o,$(SRCS))
DEPS := $(OBJS:.o=.d)

CXXFLAGS ?=
CXXFLAGS += -std=c++17 -O2 -g -fno-omit-frame-pointer -fsanitize=address
CXXFLAGS += -I$(SCRIPT_DIR)/include
CXXFLAGS += -MMD -MP

LDFLAGS ?=
LDFLAGS += -fsanitize=address -ldl

.PHONY: all check-wrapper install clean print

all: check-wrapper $(TARGET)

check-wrapper:
	@if [[ ! -x "$(CXX)" ]]; then \
		echo "Error: required wrapper '$(CXX)' not found or not executable" >&2; \
		exit 1; \
	fi

$(BUILD_DIR):
	@mkdir -p "$(BUILD_DIR)"

$(BUILD_DIR)/%.o: $(SCRIPT_DIR)/src/%.cpp | $(BUILD_DIR)
	"$(CXX)" $(CXXFLAGS) -c "$<" -o "$@"

$(TARGET): $(OBJS)
	"$(CXX)" $(OBJS) $(LDFLAGS) -o "$@"

install: all
	@mkdir -p "$(INSTALL_DIR)"
	install -m 0755 "$(TARGET)" "$(INSTALL_TARGET)"
	@echo "Done: $(INSTALL_TARGET)"

clean:
	rm -rf "$(BUILD_DIR)"

print:
	@echo "SCRIPT_DIR=$(SCRIPT_DIR)"
	@echo "APP_DIR=$(APP_DIR)"
	@echo "CXX=$(CXX)"
	@echo "SRCS=$(SRCS)"

-include $(DEPS)
