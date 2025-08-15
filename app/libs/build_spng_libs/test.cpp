

#include <dlfcn.h>
#include <stdio.h>
#include <fstream>
#include <iostream>
#include <string.h>
#include <vector>
#include <assert.h>

using namespace std;

unsigned char shared_memory[65536];
unsigned char error_tags[1000];
int THE_RET_VALUE = 0;
int counter = 0;

extern "C" {
 void __sanitizer_cov_trace_semantic(uint32_t tagId) { shared_memory[tagId] = 1; }
 unsigned char* __sanitizer_cov_trace_semantic_entry() { return shared_memory;}
 void __sanitizer_cov_trace_semantic_init() { memset(shared_memory,0,65536); }
 void __sanitizer_cov_trace_error(uint32_t tagId) { 
  error_tags[tagId%1000] = 1;}
 void __sanitizer_cov_trace_error_init() { memset(error_tags, 0, 1000); }
 unsigned char* __sanitizer_cov_trace_error_entry() { return error_tags;}
 int GetOriginalRetValue() {return THE_RET_VALUE; }
}

typedef int (*fp_t)(int argc, char *argv[]);

typedef int (*interface_t)(const uint8_t *data, size_t size);

static interface_t function_ptr_v2 = NULL;  // old version
static interface_t function_ptr_v3 = NULL; // new version

void *get_interface_fn(void *handle, const char *libpath, const char *fname) {
  void *generic_fp;
  char *error;

  handle = dlopen(libpath,  RTLD_LOCAL|RTLD_LAZY);
  if (!handle) {
    printf("cannot load library: %s\n", dlerror());
    return NULL;
  }

  generic_fp = dlsym(handle, fname);
  if ((error = dlerror()) != NULL)  {
    printf("cannot resolve function: %s\n", error);
    return NULL;
  }

  return generic_fp;
}


int feed_input(int argc, char *argv[]){
  if (argc!=2)
    {
      printf("Usage: %s [file] \n", *argv);
      return 0;
    }
  std::ifstream in(argv[1]);
  in.seekg(0, in.end);
  size_t length = in.tellg();
  in.seekg (0, in.beg);
  std::vector<char> bytes(length);
  in.read(bytes.data(), bytes.size());
  // if (size<14 && (data[0]!=0x42 || data[1]!=0x4d))
  //   return -1;

  void *handle1;
  void *handle2;
  const char * libpath1 = "./libspng_old.so";
  const char * libpath2 = "./libspng_new.so";

  function_ptr_v2 = (interface_t) get_interface_fn(handle1, libpath1,"libspng_parse");
  function_ptr_v3 = (interface_t) get_interface_fn(handle2, libpath2,"libspng_parse");

  printf("func old:%p func new:%p \n", function_ptr_v2, function_ptr_v3);

  assert(function_ptr_v2 !=NULL && function_ptr_v3 !=NULL);
  assert(function_ptr_v2 != function_ptr_v3);

  int ret2 = function_ptr_v2(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());
  int ret3 = function_ptr_v3(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());

  printf("ret_old:%d, ret_new:%d \n", ret2, ret3);
  return 0;
}

int main(int argc, char *argv[]) {
    feed_input(argc, argv);
    return 0;
}