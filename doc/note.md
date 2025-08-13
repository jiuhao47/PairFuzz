# Diffray 开发笔记

## 1. 现状与问题

1. **DiffExecutor 对 executor 没有限制**  
   - [x] 可用 ForkServer，但 ForkServer 疑似不支持细粒度 return value（仅 ok/crash/timeout），实际使用 InProcessExecutor。

2. **libpng/libspng 适配 Rust**  
   - [x] 已修改为 Rust 模式，推荐 Harness + 静态编译链接（可参考 inprocess 下 libpng 的编译）。
   - [x] 验证过旧编译模式在 Rust 下的适应性（已改为静态库）。
   - [ ] 需确认依赖库（如 libzlib）是否需自主编译。

3. **libpng harness 编译方式**  
   - 需了解 libpng harness 的编译方法，并仿写可参数化/配置化的 build 脚本，或将其分离。
   - 示例（justfile）：
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

4. **multimap**  
   - LibAFL 内置 multimap 有 bug，暂不可用。
   - 这个其实说法是路径组合（类似joint），在我们的实现范围内

5. **diff 去重**  
   - 之前 diff 没有去重，导致大量重复，现已通过 `feedback_and_fast!` 解决。

6. **diff 模式时间收集**  
   - libafl 未实现 diff 模式的时间收集，已实现空实现。
     > 见 `LibAFL/crates/libafl/src/observers/mod.rs:395`
   - [ ] 可以尝试实现这个时间收集
  
7. **时间与执行次数控制**  
   - 控制尚不够精确。

8. **CRC 校验和**  
   - [ ] 是否需要 patch，待确认。

---

## 2. 组件说明

1. **bitmap**  
   - 非 multimap，采用 `StdMapObserver*2`、`DifferentialAFLMapSwapObserver`、`HitcountsMapObserver`。
   - 本质：两倍大小位图空间，两个指针分别指向前后半部分，Swap 负责移动指针。

2. **feedback**  
   - `MaxMapFeedback`：用于最大化映射 count（发现更多路径/更频繁路径）。
     - [ ] 可基于此实现 NEZHA 策略。
     - [ ] 可实现缩减文件大小、执行时间的反馈（执行时间有 bug）。

3. **corpus**  
   - 在内存，diff 结果存磁盘。

4. **Monitor**  
   - 采用 `OnDiskJson` 和 `toml` 两种格式。
     - [ ] 可考虑删除 toml，仅保留 json（详细用于数据分析，简明用于人工查看）。

5. **EventManager**  
   - 使用 Simple，单进程最优。

6. **scheduler**  
   - 当前为 queue，可选：
     - `RandScheduler` / `StdScheduler`
     - `WeightedScheduler` / `StdWeightedScheduler`
     - `PowerQueueScheduler`
     - `MinimizerScheduler`
     - `ProbabilitySamplingScheduler`
     - `TuneableScheduler`
     - `MergeScheduler`（后两者未必有用）
     - [ ] libafl 疑似无 entropy
       > LibFuzzer 兼容层为两层调度器：外层 `IndexesLenTimeMinimizerScheduler`（优先快小样本），内层 `StdWeightedScheduler`（AFL++ 风格，功率调度）

7. **Fuzzer**  
   - 使用 Std

8. **generator**  
   - `RandPrintablesGenerator`

9. **mutator**  
   - `HavocScheduledMutator`（std）

10. **stage**  
    - `StdMutationalStage`
    - 变异类：
      - `StdMutationalStage`：标准变异
      - `MultiMutationalStage`：多输入变异
      - `StdPowerMutationalStage`：功率变异
      - `StdTMinMutationalStage`：最小化变异
    - 生成类：
      - `GenStage`：生成新输入
    - 校准/分析类：
      - `CalibrationStage`：校准
      - `TracingStage`：跟踪
      - `GeneralizationStage`：泛化
    - 同步/统计类：
      - `AflStatsStage`：AFL 统计
      - `SyncStage`：同步
      - `VerifyTimeoutsStage`：超时验证
    - 专门化：
      - `ConcolicTracingStage`：符号执行跟踪
      - `SimpleConcolicMutationalStage`：简单符号执行变异
