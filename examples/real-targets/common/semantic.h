#ifndef PAIRFUZZ_REAL_TARGET_SEMANTIC_H
#define PAIRFUZZ_REAL_TARGET_SEMANTIC_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PAIRFUZZ_SEMANTIC_ABI_VERSION 1u
#define PAIRFUZZ_INPUT_LIMIT (1024u * 1024u)
#define PAIRFUZZ_DIAGNOSTIC_LIMIT 256u
#define PAIRFUZZ_FACTS_LIMIT 512u

enum pairfuzz_result_class {
  PAIRFUZZ_RESULT_ACCEPTED = 0,
  PAIRFUZZ_RESULT_REJECTED = 1,
  PAIRFUZZ_RESULT_UNSUPPORTED = 2,
  PAIRFUZZ_RESULT_RESOURCE_LIMIT = 3,
  PAIRFUZZ_RESULT_ADAPTER_ERROR = 4,
};

struct pairfuzz_operation_result {
  enum pairfuzz_result_class class_id;
  uint32_t width;
  uint32_t height;
  uint8_t *normalized;
  size_t normalized_len;
  char diagnostic[PAIRFUZZ_DIAGNOSTIC_LIMIT];
  char facts_json[PAIRFUZZ_FACTS_LIMIT];
};

void pairfuzz_operation_result_init(struct pairfuzz_operation_result *result);
void pairfuzz_result_release(struct pairfuzz_operation_result *result);
uint64_t pairfuzz_pack_result(enum pairfuzz_result_class class_id,
                            uint64_t payload);
uint64_t pairfuzz_xxh64(const uint8_t *bytes, size_t length);
uint64_t pairfuzz_semantic_code(const struct pairfuzz_operation_result *result,
                               uint8_t schema);
int pairfuzz_canonical_bytes(const struct pairfuzz_operation_result *result,
                            uint8_t schema, uint8_t **output,
                            size_t *output_length);
const char *pairfuzz_result_class_name(enum pairfuzz_result_class class_id);

#ifdef __cplusplus
}
#endif

#endif
