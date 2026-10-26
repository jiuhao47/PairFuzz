#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "cJSON.h"
#include "target_io.h"

#ifdef DIFFRAY_INIT
DIFFRAY_INIT();
#endif

int cjson_parse(const uint8_t *data, size_t size) {
    if (size == 0) {
        return 100;
    }
    if (memchr(data, '\0', size) != NULL) {
        return 1;
    }

    size_t first = 0;
    while (first < size && is_json_whitespace(data[first])) {
        first++;
    }
    if (first == size || data[first] != '{') {
        return 1;
    }

    char *input = (char *)malloc(size + 1U);
    if (input == NULL) {
        return 1;
    }
    memcpy(input, data, size);
    input[size] = '\0';

    const char *end = NULL;
    cJSON *root = cJSON_ParseWithLengthOpts(input, size + 1U, &end, 0);
    int result = 1;
    if (root != NULL && cJSON_IsObject(root) && end != NULL && end >= input) {
        size_t consumed = (size_t)(end - input);
        if (consumed <= size) {
            result = 0;
            for (size_t index = consumed; index < size; index++) {
                if (!is_json_whitespace((uint8_t)input[index])) {
                    result = 1;
                    break;
                }
            }
        }
    }
    cJSON_Delete(root);
    free(input);
    return result;
}

int main(void) {
#ifdef DIFFRAY_INIT
    if (getenv("__AFL_SHM_ID") != NULL) {
        DIFFRAY_RUN(cjson_parse);
    }
#endif
    return run_standalone(cjson_parse);
}
