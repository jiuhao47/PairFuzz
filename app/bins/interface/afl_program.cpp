// Clean, minimal, parameterized runtime loader + DIFFRAY harness
#include <dlfcn.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <unistd.h>

// extern "C" volatile void *__afl_fuzz_ptr = nullptr;

// Generic parse function signature expected from loaded modules
typedef int (*parse_fn_t)(const uint8_t *, size_t);

static parse_fn_t fn_a_ptr = nullptr;
static void *handle_a = nullptr;
static parse_fn_t fn_b_ptr = nullptr;
static void *handle_b = nullptr;

static std::string exe_dir(void) {
  char buf[PATH_MAX];
  ssize_t len = readlink("/proc/self/exe", buf, sizeof(buf) - 1);
  if (len <= 0)
    return std::string(".");
  buf[len] = '\0';
  char *p = strrchr(buf, '/');
  if (p)
    *p = '\0';
  return std::string(buf);
}

static void load_once(void **handle, parse_fn_t *fn_ptr,
                      const std::string &lib_name, const char *fn_name) {
  if (*fn_ptr)
    return;
  std::string candidate = lib_name;
  if (lib_name.find('/') == std::string::npos)
    candidate = exe_dir() + "/" + lib_name;
  *handle = dlopen(candidate.c_str(), RTLD_LOCAL | RTLD_NOW);
  if (!*handle)
    *handle = dlopen(lib_name.c_str(), RTLD_LOCAL | RTLD_NOW);
  if (!*handle) {
    fprintf(stderr, "dlopen %s failed: %s\n", lib_name.c_str(), dlerror());
    abort();
  }
  dlerror();
  *fn_ptr = (parse_fn_t)dlsym(*handle, fn_name);
  char *err = dlerror();
  if (err) {
    fprintf(stderr, "dlsym %s failed: %s\n", fn_name, err);
    abort();
  }
}

// runtime parameters (must be provided via command-line)
static std::string g_prog_a = "";
static std::string g_prog_b = "";
static std::string g_func_a = "";
static std::string g_func_b = "";

static void load_prog_a_once(void) {
  load_once(&handle_a, &fn_a_ptr, g_prog_a, g_func_a.c_str());
}
static void load_prog_b_once(void) {
  load_once(&handle_b, &fn_b_ptr, g_prog_b, g_func_b.c_str());
}

int cmp(const uint8_t *data, size_t size) {
  load_prog_a_once();
  load_prog_b_once();
  int ret_a = fn_a_ptr(data, size);
  int ret_b = fn_b_ptr(data, size);
  if ((ret_a == 0 && ret_b != 0) || (ret_a != 0 && ret_b == 0))
    abort();
  return 0;
}

int main(int argc, char **argv) {
  for (int i = 1; i < argc; ++i) {
    const char *arg = argv[i];
    if (strncmp(arg, "--prog-a=", 9) == 0)
      g_prog_a = std::string(arg + 9);
    else if (strncmp(arg, "--func-a=", 9) == 0)
      g_func_a = std::string(arg + 9);
    else if (strncmp(arg, "--prog-b=", 9) == 0)
      g_prog_b = std::string(arg + 9);
    else if (strncmp(arg, "--func-b=", 9) == 0)
      g_func_b = std::string(arg + 9);
  }
  // validate required parameters
  if (g_prog_a.empty() || g_func_a.empty() || g_prog_b.empty() ||
      g_func_b.empty()) {
    fprintf(stderr,
            "Missing required parameters. Usage:\n  --prog-a=PATH "
            "--func-a=SYMBOL --prog-b=PATH --func-b=SYMBOL\nExample:\n  %s "
            "--prog-a=/path/libpng_local.so --func-a=libpng_parse "
            "--prog-b=/path/libspng_local.so --func-b=libspng_parse\n",
            (argc > 0 ? argv[0] : "program"));
    return 1;
  }
  size_t cap = 4096;
  size_t len = 0;
  unsigned char *buf = (unsigned char *)malloc(cap);

  ssize_t r;
  while ((r = read(0, buf + len, cap - len)) > 0) {
    len += r;
    if (len == cap) {
      cap *= 2;
      buf = (unsigned char *)realloc(buf, cap);
    }
  }
  cmp(buf, len);
  free(buf);

  return 0;
}