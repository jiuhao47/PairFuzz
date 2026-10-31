# 参与 PairFuzz 开发

感谢关注 PairFuzz。本仓库接受能够保持双目标执行、结果可复现和第三方边界清晰的改进。

## 开始前

开发环境与最小运行用例见 [README](README.md)。请使用仓库固定的 Rust 1.93.1、
LibAFL 0.16.1 gitlink 和两份 `Cargo.lock`；不要用本机另一个 LibAFL checkout 或未锁定
依赖替代它们。

建议从独立分支或 worktree 工作。提交前确认改动不包含 `run/` 中的构建产物、下载源码、
语料、私有配置、凭据或受保护的报名材料。

## 常用验证

Rust 格式与核心测试：

```bash
cargo fmt --all --check
cargo check --all-targets --workspace --locked
cargo test --workspace --locked
```

Python 与仓库工作流：

```bash
python3 -m unittest discover -s tests -p 'test_*.py' -v
just check
```

`just check` 构建并验证合成目标和 cJSON/mjson 最小用例。大型原生目标使用单独门禁：

```bash
just real-targets-check
```

后者会下载锁定源码并构建 PNG、BMP、X.509 的 ASan/release 目标，需要网络、Docker
可用的 Clang 工具链、数 GiB 临时磁盘和较长时间。只改 Rust 或普通文档时不应无理由
重复该门禁；涉及原生契约、适配器、来源锁、编译 profile 或结果导出时必须运行。

## 修改目标适配器

- 两个实现必须消费同一字节输入，并遵守对应 `examples/real-targets/*/contract.json`。
- 热路径不得写文件或依赖 stdout；语义类别与规范化指纹通过完整 `u64` 返回。
- 每次执行前必须清理可复用状态，不能把上一输入的返回值、像素或 DER 带入下一次。
- `accepted/rejected/unsupported/resource-limit/adapter-error` 的分类必须两侧可比较；
  adapter-error 不能伪装成普通拒绝。
- 新 fixture 应有独立、手工核对的期望值，覆盖成功、拒绝、资源边界和同进程
  valid→invalid→valid。
- 新来源必须固定 tag/commit、归档 SHA-256、许可证和验证链接；不得悄悄回退到系统库。
- 保存候选仍遵守“每个差分覆盖路径保存首个语义差异”，并由 manifest 指定二进制
  独立回放三次。

## 实验结果与复现

不得手工修改原始 campaign 数据来改善结果。聚合记录必须保留：

- 源码、语料和目标哈希；
- 实际墙钟、CPU、profile、campaign seed 与执行次数；
- 保存、稳定、不稳定和 replay 完整度；
- 零发现、早退和失败状态。

短时 smoke 只能说明工作流闭合，不能证明“没有差异”或用于跨操作性能排名。候选行为
差异也不能未经分析就称为漏洞。

## 提交与审查

保持提交单一、可审查，并使用真实开发历史。推荐在实现前增加能观察到失败的回归测试，
随后完成最小改动并运行相关门禁。不要伪造提交、作者、实验时间或测试记录。

暂存时列出精确路径，例如：

```bash
git add src/feedbacks/semantic.rs tests/test_semantic_runtime.py
```

不要在含有本地构建、报名表或受保护材料的工作区使用 `git add -A`。提交说明应描述行为
变化，而不是只写“update”或“fix”。

## 许可证与来源

PairFuzz 原创代码与根目录说明采用 [MIT](LICENSE) 许可证；第三方内容继续遵循各自许可证。
提交贡献时，请确保你有权按对应许可证提供该贡献。
第三方组件及其许可证见 [THIRD_PARTY.md](THIRD_PARTY.md) 和机器可读锁文件。
提交第三方代码、数据、夹具或借鉴实现时必须同时说明来源和适用许可。
