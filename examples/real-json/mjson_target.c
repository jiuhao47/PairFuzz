#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "mjson.h"
#include "target_io.h"

#ifdef DIFFRAY_INIT
DIFFRAY_INIT();
#endif

static int is_json_space(uint8_t byte) {
    return byte == ' ' || byte == '\t' || byte == '\n' || byte == '\r';
}

int mjson_parse(const uint8_t *data, size_t size) {
    if (size == 0) {
        return 100;
    }
    if (size > INT_MAX || memchr(data, '\0', size) != NULL) {
        return 1;
    }

    size_t first = 0;
    while (first < size && is_json_space(data[first])) {
        first++;
    }
    if (first == size || data[first] != '{') {
        return 1;
    }

    int consumed = mjson((const char *)data, (int)size, NULL, NULL);
    if (consumed < 0) {
        return 1;
    }
    for (size_t index = (size_t)consumed; index < size; index++) {
        if (!is_json_space(data[index])) {
            return 1;
        }
    }
    return 0;
}

int main(void) {
#ifdef DIFFRAY_INIT
    if (getenv("__AFL_SHM_ID") != NULL) {
        DIFFRAY_RUN(mjson_parse);
    }
#endif
    return run_standalone(mjson_parse);
}
