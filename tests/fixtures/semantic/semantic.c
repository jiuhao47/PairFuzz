#include <stdint.h>
#include <stdlib.h>
#include <unistd.h>

int diffray_result_init(void);
void diffray_publish_result(uint64_t value);

int main(int argc, char **argv) {
    if (diffray_result_init() != 0) {
        return 90;
    }
    uint64_t value = argc > 1 ? strtoull(argv[1], NULL, 0) : UINT64_C(0x3123456789abcdef);
    unsigned long repeats = argc > 2 ? strtoul(argv[2], NULL, 10) : 1;
    static const char begin[] = "DIFFRAY_PUBLISH_BEGIN\n";
    static const char end[] = "DIFFRAY_PUBLISH_END\n";
    (void)write(STDERR_FILENO, begin, sizeof(begin) - 1U);
    for (unsigned long i = 0; i < repeats; ++i) {
        diffray_publish_result(value);
    }
    (void)write(STDERR_FILENO, end, sizeof(end) - 1U);
    return 0;
}
