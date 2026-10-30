#ifndef PAIRFUZZ_REAL_TARGET_OPERATION_H
#define PAIRFUZZ_REAL_TARGET_OPERATION_H

#include "semantic.h"

#ifdef __cplusplus
extern "C" {
#endif

int target_init(void);
const char *target_operation(void);
uint8_t target_schema(void);
void target_execute(const uint8_t *input, size_t length,
                    struct pairfuzz_operation_result *result);
void target_cleanup(void);

#if defined(__GNUC__) || defined(__clang__)
int target_replay_facts(const uint8_t *input, size_t length,
                        char output[PAIRFUZZ_FACTS_LIMIT])
    __attribute__((weak));
#endif

#ifdef __cplusplus
}
#endif

#endif
