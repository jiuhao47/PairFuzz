#include "harness_io.h"
#include "operation.h"

#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static uint8_t active_schema;

static void json_string(const char *value) {
  putchar('"');
  for (const unsigned char *cursor = (const unsigned char *)value; *cursor;
       ++cursor) {
    switch (*cursor) {
      case '"':
        fputs("\\\"", stdout);
        break;
      case '\\':
        fputs("\\\\", stdout);
        break;
      case '\n':
        fputs("\\n", stdout);
        break;
      case '\r':
        fputs("\\r", stdout);
        break;
      case '\t':
        fputs("\\t", stdout);
        break;
      default:
        if (*cursor < 0x20) {
          printf("\\u%04x", *cursor);
        } else {
          putchar(*cursor);
        }
    }
  }
  putchar('"');
}

static int read_bounded(FILE *input, uint8_t **bytes, size_t *length) {
  uint8_t *buffer = malloc(DIFFRAY_INPUT_LIMIT + 1u);
  if (buffer == NULL) return -1;
  size_t used = 0;
  while (used < DIFFRAY_INPUT_LIMIT + 1u) {
    size_t count = fread(buffer + used, 1, DIFFRAY_INPUT_LIMIT + 1u - used,
                         input);
    used += count;
    if (count == 0) {
      if (ferror(input)) {
        free(buffer);
        return -1;
      }
      break;
    }
  }
  *bytes = buffer;
  *length = used;
  return 0;
}

static void execute_operation(const uint8_t *input, size_t length,
                              struct diffray_operation_result *result) {
  diffray_operation_result_init(result);
  if (length > DIFFRAY_INPUT_LIMIT) {
    result->class_id = DIFFRAY_RESULT_RESOURCE_LIMIT;
    strcpy(result->diagnostic, "input exceeds 1 MiB limit");
    return;
  }
  target_execute(input, length, result);
  if (result->class_id != DIFFRAY_RESULT_ACCEPTED) {
    free(result->normalized);
    result->normalized = NULL;
    result->normalized_len = 0;
    result->width = 0;
    result->height = 0;
  }
}

static uint64_t execute_for_fuzzer(const uint8_t *input, size_t length) {
  struct diffray_operation_result result;
  execute_operation(input, length, &result);
  uint64_t code = diffray_semantic_code(&result, active_schema);
  diffray_result_release(&result);
  return code;
}

static int write_file(const char *path, const uint8_t *bytes, size_t length) {
  FILE *output = fopen(path, "wb");
  if (output == NULL) return -1;
  int failed = length != 0 && fwrite(bytes, 1, length, output) != length;
  if (fclose(output) != 0) failed = 1;
  return failed ? -1 : 0;
}

static int replay(const char *input_path, const char *output_path) {
  FILE *input = fopen(input_path, "rb");
  if (input == NULL) {
    fprintf(stderr, "cannot open replay input %s: %s\n", input_path,
            strerror(errno));
    return 3;
  }
  uint8_t *bytes = NULL;
  size_t length = 0;
  int read_status = read_bounded(input, &bytes, &length);
  fclose(input);
  if (read_status != 0) {
    fprintf(stderr, "cannot read replay input %s\n", input_path);
    return 3;
  }

  struct diffray_operation_result result;
  execute_operation(bytes, length, &result);
  if (result.class_id == DIFFRAY_RESULT_ACCEPTED && target_replay_facts != NULL &&
      target_replay_facts(bytes, length, result.facts_json) != 0) {
    strcpy(result.facts_json, "{\"version\":null,\"serial_hex\":null,\"subject\":null,\"issuer\":null,\"not_before\":null,\"not_after\":null,\"public_key_algorithm_oid\":null,\"signature_algorithm_oid\":null}");
  }
  free(bytes);
  uint64_t code = diffray_semantic_code(&result, active_schema);
  unlink(output_path);
  if (result.class_id == DIFFRAY_RESULT_ACCEPTED) {
    uint8_t *canonical = NULL;
    size_t canonical_length = 0;
    if (diffray_canonical_bytes(&result, active_schema, &canonical,
                                &canonical_length) != 0 ||
        write_file(output_path, canonical, canonical_length) != 0) {
      free(canonical);
      diffray_result_release(&result);
      fprintf(stderr, "cannot write normalized replay output %s\n", output_path);
      return 4;
    }
    free(canonical);
  }

  fputs("{\"schema_version\":1,\"operation\":", stdout);
  json_string(target_operation());
  fputs(",\"class\":", stdout);
  json_string(diffray_result_class_name(result.class_id));
  printf(",\"semantic_code\":\"%016" PRIx64
         "\",\"width\":%" PRIu32 ",\"height\":%" PRIu32
         ",\"normalized_length\":%zu,\"facts\":",
         code, result.width, result.height, result.normalized_len);
  if (result.facts_json[0] == '\0') {
    fputs("{}", stdout);
  } else {
    fputs(result.facts_json, stdout);
  }
  fputs(",\"diagnostic\":", stdout);
  json_string(result.diagnostic);
  fputs("}\n", stdout);
  if (result.diagnostic[0] != '\0') {
    fprintf(stderr, "%s\n", result.diagnostic);
  }
  int status = result.class_id == DIFFRAY_RESULT_ADAPTER_ERROR ? 5 : 0;
  diffray_result_release(&result);
  return status;
}

int diffray_harness_main(int argc, char **argv) {
  if (target_init() != 0) {
    fputs("target initialization failed\n", stderr);
    return 70;
  }
  active_schema = target_schema();
  if (argc == 1) {
#ifdef DIFFRAY_RUN_SEMANTIC
    DIFFRAY_INIT();
    DIFFRAY_RUN_SEMANTIC(execute_for_fuzzer);
#else
    uint8_t *input = NULL;
    size_t length = 0;
    if (read_bounded(stdin, &input, &length) != 0) {
      target_cleanup();
      return 3;
    }
    struct diffray_operation_result result;
    execute_operation(input, length, &result);
    free(input);
    (void)diffray_semantic_code(&result, active_schema);
    diffray_result_release(&result);
    target_cleanup();
    return 0;
#endif
  }
  int status = 2;
  if (argc == 5 && strcmp(argv[1], "--replay") == 0 &&
      strcmp(argv[3], "--normalized-output") == 0) {
    status = replay(argv[2], argv[4]);
  } else {
    fputs("usage: target [--replay INPUT --normalized-output PATH]\n", stderr);
  }
  target_cleanup();
  return status;
}

int main(int argc, char **argv) { return diffray_harness_main(argc, argv); }
