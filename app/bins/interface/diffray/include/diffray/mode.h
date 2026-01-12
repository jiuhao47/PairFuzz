#pragma once

#include <string_view>

namespace diffray {

enum class Mode {
  SingleCov,
  ComboCov,
  SinglePl,
  ComboPl,
  SingleRv,
  ComboRv,
  SingleRvCov,
};

constexpr std::string_view mode_to_string(Mode m) {
  switch (m) {
  case Mode::SingleCov:
    return "single-cov";
  case Mode::ComboCov:
    return "combo-cov";
  case Mode::SinglePl:
    return "single-pl";
  case Mode::ComboPl:
    return "combo-pl";
  case Mode::SingleRv:
    return "single-rv";
  case Mode::ComboRv:
    return "combo-rv";
  case Mode::SingleRvCov:
    return "single-rv-cov";
  }
  return "unknown";
}

bool parse_mode(std::string_view s, Mode &out);

} // namespace diffray
