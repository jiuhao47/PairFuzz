#include "diffray/args.h"

#include <cstdio>
#include <cstring>

namespace diffray {

static void print_help(const char *argv0) {
  std::fprintf(stderr,
               "Usage:\n"
               "  %s --mode=MODE --prog-a=PATH --func-a=SYMBOL --prog-b=PATH "
               "--func-b=SYMBOL\n\n"
               "Modes:\n"
               "  single-cov | combo-cov | single-pl | combo-pl | single-rv | "
               "combo-rv | single-rv-cov\n",
               (argv0 ? argv0 : "diffray_program"));
}

static bool parse_kv(const char *arg, const char *key, std::string &out) {
  const std::size_t key_len = std::strlen(key);
  if (std::strncmp(arg, key, key_len) != 0)
    return false;
  out.assign(arg + key_len);
  return true;
}

bool parse_args(int argc, char **argv, Args &out) {
  for (int i = 1; i < argc; ++i) {
    const char *arg = argv[i];
    if (!arg)
      continue;

    if (std::strcmp(arg, "--help") == 0 || std::strcmp(arg, "-h") == 0) {
      print_help(argv[0]);
      return false;
    }

    std::string v;
    if (parse_kv(arg, "--prog-a=", v)) {
      out.prog_a = v;
      continue;
    }
    if (parse_kv(arg, "--prog-b=", v)) {
      out.prog_b = v;
      continue;
    }
    if (parse_kv(arg, "--func-a=", v)) {
      out.func_a = v;
      continue;
    }
    if (parse_kv(arg, "--func-b=", v)) {
      out.func_b = v;
      continue;
    }
    if (parse_kv(arg, "--mode=", v)) {
      Mode m;
      if (!parse_mode(v, m)) {
        std::fprintf(stderr, "Unknown mode: %s\n", v.c_str());
        print_help(argv[0]);
        return false;
      }
      out.mode = m;
      continue;
    }
  }

  if (out.prog_a.empty() || out.func_a.empty() || out.prog_b.empty() ||
      out.func_b.empty()) {
    std::fprintf(stderr, "Missing required parameters.\n");
    print_help(argv[0]);
    return false;
  }
  return true;
}

} // namespace diffray
