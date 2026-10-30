#ifndef PAIRFUZZ_REAL_JSON_TARGET_IO_H
#define PAIRFUZZ_REAL_JSON_TARGET_IO_H

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define PAIRFUZZ_REAL_JSON_MAX_INPUT (1024U * 1024U)

typedef int (*pairfuzz_parse_fn)(const uint8_t *data, size_t size);

static int is_json_whitespace(uint8_t byte) {
    return byte == ' ' || byte == '\t' || byte == '\n' || byte == '\r';
}

static int run_standalone(pairfuzz_parse_fn parse) {
    uint8_t *data = (uint8_t *)malloc(PAIRFUZZ_REAL_JSON_MAX_INPUT + 1U);
    if (data == NULL) {
        return 2;
    }

    size_t size = fread(data, 1, PAIRFUZZ_REAL_JSON_MAX_INPUT + 1U, stdin);
    if (ferror(stdin)) {
        free(data);
        return 2;
    }
    if (size > PAIRFUZZ_REAL_JSON_MAX_INPUT) {
        free(data);
        return 1;
    }

    int result = parse(data, size);
    free(data);
    return result;
}

#endif
