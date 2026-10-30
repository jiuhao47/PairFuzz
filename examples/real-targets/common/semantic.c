#define XXH_INLINE_ALL
#include "xxhash.h"

#include "semantic.h"

#include <stdlib.h>
#include <string.h>

#define PAIRFUZZ_PAYLOAD_MASK UINT64_C(0x0fffffffffffffff)

static void encode_le32(uint8_t output[4], uint32_t value) {
  output[0] = (uint8_t)value;
  output[1] = (uint8_t)(value >> 8);
  output[2] = (uint8_t)(value >> 16);
  output[3] = (uint8_t)(value >> 24);
}

static void encode_le64(uint8_t output[8], uint64_t value) {
  for (unsigned int index = 0; index < 8; ++index) {
    output[index] = (uint8_t)(value >> (index * 8));
  }
}

static size_t canonical_header(const struct pairfuzz_operation_result *result,
                               uint8_t schema, uint8_t output[17]) {
  output[0] = schema;
  if (schema == 1) {
    encode_le32(output + 1, result->width);
    encode_le32(output + 5, result->height);
    encode_le64(output + 9, (uint64_t)result->normalized_len);
    return 17;
  }
  if (schema == 2) {
    encode_le64(output + 1, (uint64_t)result->normalized_len);
    return 9;
  }
  return 0;
}

void pairfuzz_operation_result_init(struct pairfuzz_operation_result *result) {
  memset(result, 0, sizeof(*result));
  result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
}

void pairfuzz_result_release(struct pairfuzz_operation_result *result) {
  free(result->normalized);
  memset(result, 0, sizeof(*result));
  result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
}

uint64_t pairfuzz_pack_result(enum pairfuzz_result_class class_id,
                            uint64_t payload) {
  if ((unsigned int)class_id > PAIRFUZZ_RESULT_ADAPTER_ERROR) {
    class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
  }
  uint64_t class_bits = ((uint64_t)class_id) << 60;
  return class_id == PAIRFUZZ_RESULT_ACCEPTED
             ? class_bits | (payload & PAIRFUZZ_PAYLOAD_MASK)
             : class_bits;
}

uint64_t pairfuzz_xxh64(const uint8_t *bytes, size_t length) {
  return XXH64(bytes, length, 0);
}

uint64_t pairfuzz_semantic_code(const struct pairfuzz_operation_result *result,
                               uint8_t schema) {
  if (result == NULL ||
      (unsigned int)result->class_id > PAIRFUZZ_RESULT_ADAPTER_ERROR) {
    return pairfuzz_pack_result(PAIRFUZZ_RESULT_ADAPTER_ERROR, 0);
  }
  if (result->class_id != PAIRFUZZ_RESULT_ACCEPTED) {
    return pairfuzz_pack_result(result->class_id, 0);
  }
  if ((result->normalized_len != 0 && result->normalized == NULL) ||
      result->normalized_len > UINT64_MAX) {
    return pairfuzz_pack_result(PAIRFUZZ_RESULT_ADAPTER_ERROR, 0);
  }
  uint8_t header[17];
  size_t header_length = canonical_header(result, schema, header);
  if (header_length == 0) {
    return pairfuzz_pack_result(PAIRFUZZ_RESULT_ADAPTER_ERROR, 0);
  }
  XXH64_state_t state;
  if (XXH64_reset(&state, 0) != XXH_OK ||
      XXH64_update(&state, header, header_length) != XXH_OK ||
      XXH64_update(&state, result->normalized, result->normalized_len) !=
          XXH_OK) {
    return pairfuzz_pack_result(PAIRFUZZ_RESULT_ADAPTER_ERROR, 0);
  }
  return pairfuzz_pack_result(PAIRFUZZ_RESULT_ACCEPTED, XXH64_digest(&state));
}

int pairfuzz_canonical_bytes(const struct pairfuzz_operation_result *result,
                            uint8_t schema, uint8_t **output,
                            size_t *output_length) {
  if (result == NULL || output == NULL || output_length == NULL ||
      result->class_id != PAIRFUZZ_RESULT_ACCEPTED ||
      (result->normalized_len != 0 && result->normalized == NULL)) {
    return -1;
  }
  uint8_t header[17];
  size_t header_length = canonical_header(result, schema, header);
  if (header_length == 0 || result->normalized_len > SIZE_MAX - header_length) {
    return -1;
  }
  size_t total = header_length + result->normalized_len;
  uint8_t *canonical = malloc(total == 0 ? 1 : total);
  if (canonical == NULL) {
    return -1;
  }
  memcpy(canonical, header, header_length);
  if (result->normalized_len != 0) {
    memcpy(canonical + header_length, result->normalized,
           result->normalized_len);
  }
  *output = canonical;
  *output_length = total;
  return 0;
}

const char *pairfuzz_result_class_name(enum pairfuzz_result_class class_id) {
  switch (class_id) {
    case PAIRFUZZ_RESULT_ACCEPTED:
      return "accepted";
    case PAIRFUZZ_RESULT_REJECTED:
      return "rejected";
    case PAIRFUZZ_RESULT_UNSUPPORTED:
      return "unsupported";
    case PAIRFUZZ_RESULT_RESOURCE_LIMIT:
      return "resource-limit";
    case PAIRFUZZ_RESULT_ADAPTER_ERROR:
    default:
      return "adapter-error";
  }
}
