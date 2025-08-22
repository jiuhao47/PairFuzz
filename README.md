# diffray

差分模糊测试工具（Rust + LibAFL）。此仓库包含驱动程序、反馈、观察器与示例 harness，用以对比两个目标实现之间的差异。

快速开始

1. 安装 Rust 工具链（stable）。
2. 构建并运行：

   cargo build -p diffray
   cargo run -p diffray -- --help

项目结构概览见 `docs/STRUCTURE.md`。

维护注意事项

- 本仓库包含一个 vendored 的 `LibAFL/` 目录；避免在未充分测试的情况下随意修改。
- 日志文件默认写入 `run/log/debug.log`，可通过 `LOG_CORPUS` 控制 `LogMapFeedback` 的启用。

如果需要，我可以进一步：
- 将 executor/feedback 构建拆分到 `src/driver` 下的单独模块
- 为日志添加 JSON 输出以方便分析
- 在 CI 中加入 `cargo check` 作为验证步骤
