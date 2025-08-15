#ifndef LIBPNG_HARNESS_H
#define LIBPNG_HARNESS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int libpng_parse(const uint8_t *data, size_t size);

#ifdef __cplusplus
}
#endif

#endif // LIBPNG_HARNESS_H
