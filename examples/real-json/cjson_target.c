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

    char *input = (char *)malloc(size + 1U);
    if (input == NULL) {
        return 1;
    }
    memcpy(input, data, size);
    input[size] = '\0';

    const char *end = NULL;
    cJSON *root = cJSON_ParseWithLengthOpts(input, size + 1U, &end, 1);
    int result = root != NULL && cJSON_IsObject(root) ? 0 : 1;
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
