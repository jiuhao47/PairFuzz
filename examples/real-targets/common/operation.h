#ifndef DIFFRAY_REAL_TARGET_OPERATION_H
#define DIFFRAY_REAL_TARGET_OPERATION_H

#include "semantic.h"

#ifdef __cplusplus
extern "C" {
#endif

int target_init(void);
const char *target_operation(void);
uint8_t target_schema(void);
void target_execute(const uint8_t *input, size_t length,
                    struct diffray_operation_result *result);
void target_cleanup(void);

#ifdef __cplusplus
}
#endif

#endif
