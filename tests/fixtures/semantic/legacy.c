#include <stdint.h>

int pairfuzz_result_init(void);

int main(void) {
    if (pairfuzz_result_init() != 0) {
        return 90;
    }
    PAIRFUZZ_RETVAL(-1);
    return 0;
}
