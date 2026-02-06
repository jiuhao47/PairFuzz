// Minimal, parameterized runtime loader + replay harness
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

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

struct MappedInput {
  const uint8_t *data;
  size_t size;
  void *mapping;
  size_t mapping_size;
};

static void unmap_input(MappedInput *in) {
  if (in->mapping && in->mapping_size) {
    munmap(in->mapping, in->mapping_size);
  }
  in->data = nullptr;
  in->size = 0;
  in->mapping = nullptr;
  in->mapping_size = 0;
}

static int map_file_readonly(const char *path, MappedInput *out) {
  out->data = nullptr;
  out->size = 0;
  out->mapping = nullptr;
  out->mapping_size = 0;

  int fd = open(path, O_RDONLY | O_CLOEXEC);
  if (fd < 0) {
    fprintf(stderr, "open %s failed: %s\n", path, strerror(errno));
    return -1;
  }

  struct stat st;
  if (fstat(fd, &st) != 0) {
    fprintf(stderr, "fstat %s failed: %s\n", path, strerror(errno));
    close(fd);
    return -1;
  }

  size_t size = 0;
  if (st.st_size > 0)
    size = (size_t)st.st_size;

  if (size == 0) {
    static const uint8_t kDummy = 0;
    out->data = &kDummy;
    out->size = 0;
    close(fd);
    return 0;
  }

  void *mapping = mmap(nullptr, size, PROT_READ, MAP_PRIVATE, fd, 0);
  if (mapping == MAP_FAILED) {
    fprintf(stderr, "mmap %s failed: %s\n", path, strerror(errno));
    close(fd);
    return -1;
  }
  close(fd);

  out->mapping = mapping;
  out->mapping_size = size;
  out->data = (const uint8_t *)mapping;
  out->size = size;
  return 0;
}

static bool logical_diff(int ret_a, int ret_b) {
  return ((ret_a == 0) != (ret_b == 0));
}

int main(int argc, char **argv) {
  const char *input_path = nullptr;

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
    else if (strncmp(arg, "--input=", 8) == 0)
      input_path = arg + 8;
    else if (arg[0] == '-') {
      fprintf(stderr, "Unknown option: %s\n", arg);
      return 1;
    } else {
      if (input_path) {
        fprintf(stderr, "Multiple input paths provided\n");
        return 1;
      }
      input_path = arg;
    }
  }

  if (g_prog_a.empty() || g_func_a.empty() || g_prog_b.empty() ||
      g_func_b.empty() || !input_path) {
    fprintf(stderr,
            "Missing required parameters. Usage:\n"
            "  %s --prog-a=PATH --func-a=SYMBOL --prog-b=PATH --func-b=SYMBOL "
            "--input=FILE\n"
            "  %s --prog-a=PATH --func-a=SYMBOL --prog-b=PATH --func-b=SYMBOL "
            "FILE\n",
            (argc > 0 ? argv[0] : "replay_program"),
            (argc > 0 ? argv[0] : "replay_program"));
    return 1;
  }

  load_prog_a_once();
  load_prog_b_once();

  MappedInput in;
  if (map_file_readonly(input_path, &in) != 0) {
    return 2;
  }

  int ret_a = fn_a_ptr(in.data, in.size);
  int ret_b = fn_b_ptr(in.data, in.size);
  bool diff = logical_diff(ret_a, ret_b);

  printf("[DIFF]: %s\n", diff ? "Yes" : "No");

  unmap_input(&in);
  return 0;
}
