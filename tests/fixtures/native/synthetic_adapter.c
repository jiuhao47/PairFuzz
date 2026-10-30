#include "operation.h"

#include <stdlib.h>
#include <string.h>

int target_init(void) { return 0; }

const char *target_operation(void) { return "synthetic.bytes.normalize"; }

uint8_t target_schema(void) { return 1; }

void target_execute(const uint8_t *input, size_t length,
                    struct pairfuzz_operation_result *result) {
  if (length == 0 || input[0] == 'R') {
    result->class_id = PAIRFUZZ_RESULT_REJECTED;
    result->width = 999;
    result->height = 999;
    return;
  }
  if (input[0] == 'E') {
    result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "synthetic \"error\"");
    return;
  }
  result->normalized = malloc(4);
  if (result->normalized == NULL) {
    result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "allocation failed");
    return;
  }
  result->class_id = PAIRFUZZ_RESULT_ACCEPTED;
  result->width = 1;
  result->height = 1;
  result->normalized_len = 4;
  result->normalized[0] = length > 1 ? input[1] : 1;
  result->normalized[1] = 2;
  result->normalized[2] = 3;
  result->normalized[3] = 255;
}

void target_cleanup(void) {}
