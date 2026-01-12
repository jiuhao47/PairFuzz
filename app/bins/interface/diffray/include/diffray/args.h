#pragma once

#include <string>

#include "diffray/mode.h"

namespace diffray {

struct Args {
  Mode mode = Mode::SingleCov;
  std::string prog_a;
  std::string prog_b;
  std::string func_a;
  std::string func_b;
};

bool parse_args(int argc, char **argv, Args &out);

} // namespace diffray
