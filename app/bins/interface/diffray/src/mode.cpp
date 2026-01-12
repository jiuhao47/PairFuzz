#include "diffray/mode.h"

namespace diffray {

bool parse_mode(std::string_view s, Mode &out) {
  if (s == "single-cov") {
    out = Mode::SingleCov;
    return true;
  }
  if (s == "combo-cov") {
    out = Mode::ComboCov;
    return true;
  }
  if (s == "single-pl") {
    out = Mode::SinglePl;
    return true;
  }
  if (s == "combo-pl") {
    out = Mode::ComboPl;
    return true;
  }
  if (s == "single-rv") {
    out = Mode::SingleRv;
    return true;
  }
  if (s == "combo-rv") {
    out = Mode::ComboRv;
    return true;
  }
  if (s == "single-rv-cov") {
    out = Mode::SingleRvCov;
    return true;
  }
  return false;
}

} // namespace diffray
