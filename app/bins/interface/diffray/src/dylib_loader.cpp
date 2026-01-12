#include "diffray/dylib_loader.h"

#include <dlfcn.h>

#include <cstdio>
#include <cstdlib>

namespace diffray {

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
}

int DylibLoader::call(const std::uint8_t *data, std::size_t size) {
  ensure_loaded();
  return fn_(data, size);
}

} // namespace diffray
