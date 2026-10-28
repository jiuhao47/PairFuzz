#include <cstdint>
#include <cstdlib>

extern "C" int diffray_result_init(void);
extern "C" void diffray_publish_result(std::uint64_t value);

int main(int argc, char **argv) {
    if (diffray_result_init() != 0) {
        return 90;
    }
    const auto value = argc > 1 ? std::strtoull(argv[1], nullptr, 0)
                                : UINT64_C(0x3123456789abcdef);
    diffray_publish_result(value);
    return 0;
}
