#include <stdint.h>

int diffray_result_init(void);

int main(void) {
    if (diffray_result_init() != 0) {
        return 90;
    }
    DIFFRAY_RETVAL(-1);
    return 0;
}
