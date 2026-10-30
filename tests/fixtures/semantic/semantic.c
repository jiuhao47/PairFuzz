#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int pairfuzz_result_init(void);
void pairfuzz_publish_result(uint64_t value);

int main(int argc, char **argv) {
    if (pairfuzz_result_init() != 0) {
        return 90;
    }
    if (argc > 1 && strcmp(argv[1], "check-env") == 0) {
        const char *role = getenv("__PAIRFUZZ_RV_ROLE");
        const char *unrelated = getenv("PAIRFUZZ_TEST_UNRELATED");
        if (role == NULL || strcmp(role, "A") != 0 ||
            getenv("__PAIRFUZZ_RV_SHM_ID_A") == NULL ||
            getenv("__PAIRFUZZ_RV_SHM_ID_A_SIZE") == NULL ||
            getenv("__PAIRFUZZ_RV_SHM_ID_B") != NULL ||
            getenv("__PAIRFUZZ_RV_SHM_ID_B_SIZE") != NULL ||
            getenv("__DIFF" "RAY_RV_ROLE") != NULL ||
            getenv("__DIFF" "RAY_RV_SHM_ID_A") != NULL ||
            getenv("__DIFF" "RAY_RV_SHM_ID_B") != NULL ||
            unrelated == NULL || strcmp(unrelated, "preserved") != 0) {
            return 91;
        }
        return 0;
    }
    uint64_t value = argc > 1 ? strtoull(argv[1], NULL, 0) : UINT64_C(0x3123456789abcdef);
    unsigned long repeats = argc > 2 ? strtoul(argv[2], NULL, 10) : 1;
    static const char begin[] = "PAIRFUZZ_PUBLISH_BEGIN\n";
    static const char end[] = "PAIRFUZZ_PUBLISH_END\n";
    (void)write(STDERR_FILENO, begin, sizeof(begin) - 1U);
    for (unsigned long i = 0; i < repeats; ++i) {
        pairfuzz_publish_result(value);
    }
    (void)write(STDERR_FILENO, end, sizeof(end) - 1U);
    return 0;
}
