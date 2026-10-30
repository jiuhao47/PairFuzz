#include "operation.h"
#include "semantic.h"

#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint8_t *read_stdin(size_t *length) {
  size_t capacity = 1024;
  uint8_t *buffer = malloc(capacity);
  *length = 0;
  while (!feof(stdin)) {
    if (*length == capacity) {
      capacity *= 2;
      buffer = realloc(buffer, capacity);
      if (buffer == NULL) return NULL;
    }
    *length += fread(buffer + *length, 1, capacity - *length, stdin);
    if (ferror(stdin)) {
      free(buffer);
      return NULL;
    }
  }
  return buffer;
}

int main(int argc, char **argv) {
  if (argc < 2) return 2;
  if (strcmp(argv[1], "pack") == 0 && argc == 4) {
    unsigned long class_id = strtoul(argv[2], NULL, 0);
    uint64_t payload = strtoull(argv[3], NULL, 0);
    printf("%016" PRIx64 "\n",
           pairfuzz_pack_result((enum pairfuzz_result_class)class_id, payload));
    return 0;
  }
  size_t length = 0;
  uint8_t *input = read_stdin(&length);
  if (input == NULL) return 3;
  if (strcmp(argv[1], "hash") == 0 && argc == 2) {
    printf("%016" PRIx64 "\n", pairfuzz_xxh64(input, length));
    free(input);
    return 0;
  }
  if ((strcmp(argv[1], "image") == 0 && argc == 4) ||
      (strcmp(argv[1], "der") == 0 && argc == 2)) {
    struct pairfuzz_operation_result result;
    pairfuzz_operation_result_init(&result);
    result.class_id = PAIRFUZZ_RESULT_ACCEPTED;
    result.normalized = input;
    result.normalized_len = length;
    uint8_t schema = 2;
    if (strcmp(argv[1], "image") == 0) {
      schema = 1;
      result.width = (uint32_t)strtoul(argv[2], NULL, 0);
      result.height = (uint32_t)strtoul(argv[3], NULL, 0);
    }
    printf("%016" PRIx64 "\n", pairfuzz_semantic_code(&result, schema));
    pairfuzz_result_release(&result);
    return 0;
  }
  if (strcmp(argv[1], "stale") == 0 && argc == 2) {
    struct pairfuzz_operation_result result;
    const uint8_t accepted[] = {'A', 42};
    const uint8_t rejected[] = {'R'};
    pairfuzz_operation_result_init(&result);
    target_execute(accepted, sizeof accepted, &result);
    uint64_t first = pairfuzz_semantic_code(&result, target_schema());
    pairfuzz_result_release(&result);
    pairfuzz_operation_result_init(&result);
    target_execute(rejected, sizeof rejected, &result);
    uint64_t second = pairfuzz_semantic_code(&result, target_schema());
    pairfuzz_result_release(&result);
    printf("%016" PRIx64 " %016" PRIx64 "\n", first, second);
    free(input);
    return 0;
  }
  free(input);
  return 2;
}
