#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

namespace diffray {

class DylibLoader {
public:
  using ParseFn = int (*)(const std::uint8_t *, std::size_t);

  DylibLoader() = default;
  DylibLoader(const DylibLoader &) = delete;
  DylibLoader &operator=(const DylibLoader &) = delete;

  ~DylibLoader();

  void configure(std::string lib_path, std::string symbol);
  void ensure_loaded();
  int call(const std::uint8_t *data, std::size_t size);

private:
  void *handle_ = nullptr;
  ParseFn fn_ = nullptr;
  std::string lib_path_;
  std::string symbol_;
};

} // namespace diffray
