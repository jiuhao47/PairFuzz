#pragma once

#include <cstdio>

// Minimal debug helper for the interface harness.
// We intentionally keep this lightweight and dependency-free.
#define SAYF(...)                                                                 \
  do {                                                                            \
    std::fprintf(stderr, __VA_ARGS__);                                            \
    std::fflush(stderr);                                                         \
  } while (0)
