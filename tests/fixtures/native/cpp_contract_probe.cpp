#include "semantic.h"

#include <cstdint>

int main() {
  diffray_operation_result result;
  diffray_operation_result_init(&result);
  result.class_id = DIFFRAY_RESULT_REJECTED;
  std::uint64_t code = diffray_semantic_code(&result, 1);
  diffray_result_release(&result);
  return code == UINT64_C(0x1000000000000000) ? 0 : 1;
}
