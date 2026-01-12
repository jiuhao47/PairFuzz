#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>

#include "diffray/args.h"
#include "diffray/runner.h"

// Provided by the compiler wrapper:
// - DIFFRAY_INIT()
// - DIFFRAY_RUN(fn)
DIFFRAY_INIT();

static diffray::Runner *g_runner = nullptr;

static int fuzz_entry(const std::uint8_t *data, std::size_t size) {
  if (!g_runner) {
    return 0;
  }
  return g_runner->run_one(data, size);
}

int main(int argc, char **argv) {
  diffray::Args args;
  if (!diffray::parse_args(argc, argv, args)) {
    return 1;
  }

  diffray::Runner runner(args);
  g_runner = &runner;

  DIFFRAY_RUN(fuzz_entry);
  return 0;
}
