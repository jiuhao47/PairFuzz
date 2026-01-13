#include "diffray/dylib_loader.h"

#include "debug.h"

#include <dlfcn.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>

namespace diffray {

extern "C" {
extern std::uint8_t *__afl_area_ptr __attribute__((weak));
extern std::uint32_t __afl_map_size __attribute__((weak));
}

DylibLoader::~DylibLoader() {
  if (handle_) {
    dlclose(handle_);
    handle_ = nullptr;
  }
  fn_ = nullptr;
}

void DylibLoader::configure(std::string lib_path, std::string symbol) {
  lib_path_ = std::move(lib_path);
  symbol_ = std::move(symbol);
}

void DylibLoader::ensure_loaded() {
  if (fn_)
    return;

  handle_ = dlopen(lib_path_.c_str(), RTLD_LOCAL | RTLD_NOW);
  if (!handle_) {
    std::fprintf(stderr, "dlopen %s failed: %s\n", lib_path_.c_str(),
                 dlerror());
    std::abort();
  }

  dlerror();
  fn_ = reinterpret_cast<ParseFn>(dlsym(handle_, symbol_.c_str()));
  if (char *err = dlerror()) {
    std::fprintf(stderr, "dlsym %s failed: %s\n", symbol_.c_str(), err);
    std::abort();
  }

  // Debug: check whether the loaded DSO has its own AFL globals.
  // If it does, its instrumentation may be writing to a different map than
  // the main executable, causing our snapshots to look empty.
  dlerror();
  void *sym_area = dlsym(handle_, "__afl_area_ptr");
    const char *sym_area_err_c = dlerror();
    const std::string sym_area_err = sym_area_err_c ? sym_area_err_c : "";
  dlerror();
  void *sym_size = dlsym(handle_, "__afl_map_size");
    const char *sym_size_err_c = dlerror();
    const std::string sym_size_err = sym_size_err_c ? sym_size_err_c : "";

    if (sym_area && sym_area_err.empty()) {
      // Do not dereference: some loaders/runtime setups may invalidate dlerror()
      // buffers or expose symbols in ways that make reads unsafe under ASAN.
      SAYF("[diffray] dylib %s: has __afl_area_ptr symbol at %p (main=%p)\n",
        lib_path_.c_str(), sym_area, static_cast<void *>(__afl_area_ptr));
  } else {
      SAYF("[diffray] dylib %s: no __afl_area_ptr symbol (%s)\n", lib_path_.c_str(),
        sym_area_err.empty() ? "null" : sym_area_err.c_str());
  }

    if (sym_size && sym_size_err.empty()) {
      SAYF("[diffray] dylib %s: has __afl_map_size symbol at %p (main=%u)\n",
        lib_path_.c_str(), sym_size, static_cast<unsigned>(__afl_map_size));
  } else {
      SAYF("[diffray] dylib %s: no __afl_map_size symbol (%s)\n", lib_path_.c_str(),
        sym_size_err.empty() ? "null" : sym_size_err.c_str());
  }
}

int DylibLoader::call(const std::uint8_t *data, std::size_t size) {
  ensure_loaded();
  return fn_(data, size);
}

} // namespace diffray
