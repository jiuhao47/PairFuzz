#include "semantic.h"

#include <cstdint>

int main() {
  pairfuzz_operation_result result;
  pairfuzz_operation_result_init(&result);
  result.class_id = PAIRFUZZ_RESULT_REJECTED;
  std::uint64_t code = pairfuzz_semantic_code(&result, 1);
  pairfuzz_result_release(&result);
  return code == UINT64_C(0x1000000000000000) ? 0 : 1;
}
