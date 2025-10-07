

#include <assert.h>
#include <dlfcn.h>
#include <fstream>
#include <iostream>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <vector>
using namespace std;

unsigned char shared_memory[65536];
unsigned char error_tags[1000];
int THE_RET_VALUE = 0;

extern "C" {
void __sanitizer_cov_trace_semantic(uint32_t tagId) {
  shared_memory[tagId] = 1;
}
unsigned char *__sanitizer_cov_trace_semantic_entry() { return shared_memory; }
void __sanitizer_cov_trace_semantic_init() { memset(shared_memory, 0, 65536); }
void __sanitizer_cov_trace_error(uint32_t tagId) {
  printf("error id:%d, %d\n", tagId, tagId % 1000);
  error_tags[tagId % 1000] = 1;
} // printf("error id:%d\n", tagId);
void __sanitizer_cov_trace_error_init() { memset(error_tags, 0, 1000); }
unsigned char *__sanitizer_cov_trace_error_entry() { return error_tags; }
int GetOriginalRetValue() { return THE_RET_VALUE; }
}

typedef int (*fp_t)(int argc, char *argv[]);

typedef int (*interface_t)(const uint8_t *data, size_t size);

static interface_t function_ptr_v2 = NULL; // old version
static interface_t function_ptr_v3 = NULL; // new version

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

int feed_input(int argc, char *argv[]) {

  std::ifstream in(argv[1]);
  in.seekg(0, in.end);
  size_t length = in.tellg();
  in.seekg(0, in.beg);
  std::vector<char> bytes(length);
  in.read(bytes.data(), bytes.size());

  void *handle1;
  void *handle2;
  const char *libpath1 = argv[2];

  function_ptr_v2 = (interface_t)get_interface_fn(handle1, libpath1,
                                                  "verify_cert_mem_openssl");

  printf("function_ptr_v2: %p\n", function_ptr_v2);

  int ret2 = function_ptr_v2(reinterpret_cast<const uint8_t *>(bytes.data()),
                             bytes.size());

  printf("ret: %d\n", ret2);
  return 0;
}

int main(int argc, char *argv[]) {
  if (argc != 3) {
    printf("Usage: %s [file] [lib]\n", *argv);
    return 0;
  }
  feed_input(argc, argv);
  return 0;
}
