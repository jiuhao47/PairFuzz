#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "mjson.h"
#include "target_io.h"

#ifdef PAIRFUZZ_INIT
PAIRFUZZ_INIT();
#endif

int mjson_parse(const uint8_t *data, size_t size) {
    if (size == 0) {
        return 100;
    }
    if (size > INT_MAX || memchr(data, '\0', size) != NULL) {
        return 1;
    }

    size_t first = 0;
    while (first < size && is_json_whitespace(data[first])) {
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
        if (!is_json_whitespace(data[index])) {
            return 1;
        }
    }
    return 0;
}

int main(void) {
#ifdef PAIRFUZZ_INIT
    if (getenv("__AFL_SHM_ID") != NULL) {
        PAIRFUZZ_RUN(mjson_parse);
    }
#endif
    return run_standalone(mjson_parse);
}
