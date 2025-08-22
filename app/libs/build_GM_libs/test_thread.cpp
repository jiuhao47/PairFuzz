#include <assert.h>
#include <dlfcn.h>
#include <fstream>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <vector>

typedef int (*interface_t)(const uint8_t *data, size_t size);

static interface_t function_ptr_v1 = NULL;

void *get_interface_fn(void *handle, const char *libpath, const char *fname) {
  void *generic_fp;
  char *error;

  handle = dlopen(libpath, RTLD_LOCAL | RTLD_LAZY);
  if (!handle) {
    printf("cannot load library: %s\n", dlerror());
    return NULL;
  }

  generic_fp = dlsym(handle, fname);
  if ((error = dlerror()) != NULL) {
    printf("cannot resolve function: %s\n", error);
    return NULL;
  }

  return generic_fp;
}

int main(int argc, char **argv) {
  if (argc < 3) {
    printf("Usage: %s <input_file> <library_path>\n", argv[0]);
    return 1;
  }

  std::ifstream in(argv[1]);
  in.seekg(0, in.end);
  size_t length = in.tellg();
  in.seekg(0, in.beg);
  std::vector<char> bytes(length);
  in.read(bytes.data(), bytes.size());

  void *handle1;
  const char *libpath1 = argv[2];

  interface_t function_ptr_v1 = (interface_t)get_interface_fn(handle1, libpath1, "GM_parse");

  assert(function_ptr_v1 != NULL);

  int ret = function_ptr_v1(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());
  printf("ret: %d\n", ret);
  return 0;
}
