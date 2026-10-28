#ifndef DIFFRAY_REAL_TARGET_SEMANTIC_H
#define DIFFRAY_REAL_TARGET_SEMANTIC_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DIFFRAY_SEMANTIC_ABI_VERSION 1u
#define DIFFRAY_INPUT_LIMIT (1024u * 1024u)
#define DIFFRAY_DIAGNOSTIC_LIMIT 256u
#define DIFFRAY_FACTS_LIMIT 512u

enum diffray_result_class {
  DIFFRAY_RESULT_ACCEPTED = 0,
  DIFFRAY_RESULT_REJECTED = 1,
  DIFFRAY_RESULT_UNSUPPORTED = 2,
  DIFFRAY_RESULT_RESOURCE_LIMIT = 3,
  DIFFRAY_RESULT_ADAPTER_ERROR = 4,
};

struct diffray_operation_result {
  enum diffray_result_class class_id;
  uint32_t width;
  uint32_t height;
  uint8_t *normalized;
  size_t normalized_len;
  char diagnostic[DIFFRAY_DIAGNOSTIC_LIMIT];
  char facts_json[DIFFRAY_FACTS_LIMIT];
};

void diffray_operation_result_init(struct diffray_operation_result *result);
void diffray_result_release(struct diffray_operation_result *result);
uint64_t diffray_pack_result(enum diffray_result_class class_id,
                            uint64_t payload);
uint64_t diffray_xxh64(const uint8_t *bytes, size_t length);
uint64_t diffray_semantic_code(const struct diffray_operation_result *result,
                               uint8_t schema);
int diffray_canonical_bytes(const struct diffray_operation_result *result,
                            uint8_t schema, uint8_t **output,
                            size_t *output_length);
const char *diffray_result_class_name(enum diffray_result_class class_id);

#ifdef __cplusplus
}
#endif

#endif
