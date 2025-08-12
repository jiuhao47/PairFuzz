1. DiffExecutor对executor没有限制（可以使用Forkserver）
   - [ ] ForkServer疑似不支持细粒度return value（只有ok，crash，timeout）
2. app下的libpng和libspng已经修改为适应rust的模式，rust编译模式里更推崇使用Harness+静态编译链接（可参阅inprocess下的libpng的编译）
   - [ ] 需要验证之前编译模式在rust下的适应性
   - [ ] 需要确认是否需要将依赖的库进行自主编译（如libpng的依赖libzlib等）
3. 我需要知道libpng如何编译harness的，并且仿照其写一个build（并将其参数化/配置化，或者是分离开来）
   ```just
   fuzzer feat="": (lib feat) (cxx feat)
    {{LIBAFL_CXX}} {{PROJECT_DIR}}/harness.cc \
        "{{LIBPNG_BUILD}}/.libs/libpng16.a" \
        "{{ZLIB_BUILD}}/libz.a" \
        -I"{{LIBPNG_INCLUDE}}" \
        -I"{{LIBPNG_BUILD}}" \
        -I"{{ZLIB_INCLUDE}}" \
        -L"{{ZLIB_LIB}}" \
        -o {{FUZZER_NAME}} \
        -lm -lz
   ```
4. multimap LibAFL内置的有bug，暂时用不了
5. 目前的diff没有去重设计，有大量的重复，解决了，通过feedback_and_fast!
6. libafl没有做diff模式的时间收集实现，实现了一个空
   > home/jiuhao/workspace/workshop/diffray/LibAFL/crates/libafl/src/observers/mod.rs 395