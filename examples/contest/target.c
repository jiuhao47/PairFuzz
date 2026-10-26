#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef ACCEPT_HEX
#define ACCEPT_HEX 0
#endif

#ifdef DIFFRAY_INIT
DIFFRAY_INIT();
#endif

/* This synthetic pair differs only in hexadecimal syntax acceptance. */
static int parse_number(const uint8_t *data, size_t size) {
    if (size > 0 && data[size - 1] == '\n') {
        --size;
    }
    if (size == 0 || size > 64) {
        return 1;
    }

    size_t start = 0;
    int hexadecimal = 0;
#if ACCEPT_HEX
    if (size >= 2 && data[0] == '0') {
        if (data[1] == 'x' || data[1] == 'X') {
            start = 2;
            hexadecimal = 1;
        }
    }
#endif
    if (start == size) {
        return 1;
    }
    for (size_t i = start; i < size; ++i) {
        uint8_t c = data[i];
        if (c >= '0' && c <= '9') {
            continue;
        }
        if (hexadecimal && ((c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'))) {
            continue;
        }
        return 1;
    }
    return 0;
}

int main(void) {
#ifdef DIFFRAY_INIT
    if (getenv("__AFL_SHM_ID") != NULL) {
        __AFL_INIT();
        const uint8_t *data = __AFL_FUZZ_TESTCASE_BUF;
        size_t size = __AFL_FUZZ_TESTCASE_LEN;
        int result = parse_number(data, size);
        DIFFRAY_RETVAL(result);
        return result;
    }
#endif
    /* Read one extra byte to reject oversized input without unbounded storage. */
    uint8_t data[66];
    size_t size = fread(data, 1, sizeof(data), stdin);
    if (ferror(stdin)) {
        return 2;
    }
    int result = parse_number(data, size);
    printf("%s\n", result == 0 ? "accepted" : "rejected");
    return result;
}
