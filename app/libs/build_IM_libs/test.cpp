#include <assert.h>
#include <stdint.h>
#include <unistd.h>
#include <stdbool.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <dlfcn.h>
#include<fstream>
#include<iostream>
#include <string>
#include<vector>


typedef int (*interface_t)(const uint8_t *data, size_t size);

static interface_t function_ptr_v2 = NULL; 
static interface_t function_ptr_v3 = NULL;

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


int main(int argc,char **argv) 
{ 
  std::ifstream in(argv[1]);
  in.seekg(0, in.end);
  size_t length = in.tellg();
  in.seekg (0, in.beg);
  std::vector<char> bytes(length);
  in.read(bytes.data(), bytes.size());
  void *handle1;
  void *handle2;
  const char * libpath1 = "./libparseIM_oldest.so";
  const char * libpath2 = "./libparseIM_newest.so";

  function_ptr_v2 = (interface_t) get_interface_fn(handle1, libpath1,"imagemagick_parse");
  function_ptr_v3 = (interface_t) get_interface_fn(handle2, libpath2,"imagemagick_parse");

  // printf("func old:%p func new:%p \n", function_ptr_v2, function_ptr_v3);

  assert(function_ptr_v2 !=NULL && function_ptr_v3 !=NULL);
  assert(function_ptr_v2 != function_ptr_v3);

  int ret2 = function_ptr_v2(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());
  int ret3 = function_ptr_v3(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());
  printf("ret2=%d, ret3=%d\n", ret2, ret3);
  if ((ret2==0 && ret3!=0)||(ret2!=0 && ret3==0))
    printf("%d,%d, file:%s\n", ret2,ret3,argv[1]);
  // printf("ret_old:%d, ret_new:%d \n", ret2, ret3);
  // printf("ret_new:%d \n", ret3);

  return 0; 
} 
