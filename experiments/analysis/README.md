
Plotting helper: `experiments/analysis/plot_fuzzer_stats.py`
----------------------------------------------------

仓库中提供一个小脚本用于把 fuzzer 运行生成的 `fuzzer_stats.json`（可以是一个数组，也可以是多行 JSON 流）可视化为 PNG 图像。

主要用途
- 绘制 `corpus_size`, `objective_size`, `exec_sec` 随 `run_time (secs)` 的变化。
- 绘制 `user_stats`（如果存在）并在需要时标注关键点。

基本用法
# experiments/analysis

本目录包含用于分析 fuzz 实验结果的脚本和工具，主入口为 `analysis.py`，并配套有解析与绘图模块。

主要功能

- 扫描并交互式解压（或批量解压）目标目录下未解压的 `.tar.gz`/`.tgz`/`.tar` 文件。
- 为每个任务（task）下的每个子任务（subtask）汇总 `fuzzer_stats.json`，输出 `info.json`（位于每个 task 目录内）。
- 自动识别并提取 crash：从 `subtask/diffs/` 中检测 metadata（例如 `.H_1.metadata`），若 metadata 标注 `note == "crash_any"`，则将对应文件移动到 `subtask/crashes/`。
- 为每个 subtask 生成可视化图（corpus_size、objective_size、exec_sec、user_stats），保存到 `subtask/plot/`。
- 对同一目录下多个同构任务的 `info.json` 做字段级聚合（平均值），输出 `aggregated_info.json`。

主要文件

- `analysis.py`：主脚本（CLI），集成了解压、summarize、extract-crashes、aggregate、plot 等操作。
- `fuzzer_stats_parser.py`：解析 `fuzzer_stats.json` 的工具类 `FuzzerStatsParser`（负责读取 JSON 记录并生成时间序列）。
- `plot_fuzzer_stats.py`：绘图工具，提供 `FuzzerStatsPlotter` 类，可作为独立脚本使用或被 `analysis.py` 调用。

依赖

- Python 3.8+
- numpy
- matplotlib

（推荐在项目的虚拟环境或容器中安装依赖）

使用示例

1) 交互式扫描并解压未解压的 tar 包：

```bash
python3 experiments/analysis/analysis.py /path/to/parent_dir
```

2) 非交互全部解压：

```bash
python3 experiments/analysis/analysis.py /path/to/parent_dir --all
```

3) 为每个 task 生成 `info.json`（summarize）并为每个 subtask 生成绘图（plot）：

```bash
python3 experiments/analysis/analysis.py /path/to/parent_dir --summarize --plot
```

- 结果：每个 task 下会生成 `info.json`，每个 subtask 下会生成 `plot/` 目录，包含若干 PNG（`corpus_size.png`, `objective_size.png`, `exec_sec.png`, `user_stats.png`）。默认注释启用（annotate），并标注 top-5 峰值。

4) 仅提取 crashes（移动文件）：

```bash
python3 experiments/analysis/analysis.py /path/to/parent_dir --extract-crashes
```

5) 聚合同一目录下多个 task 的 `info.json`（按样本平均）：

```bash
python3 experiments/analysis/analysis.py /path/to/parent_dir --aggregate
```

- 聚合结果写入 `/path/to/parent_dir/aggregated_info.json`。

`analysis.py` 常用选项

- `target`（位置参数）：目标父目录。
- `--all`：非交互解压所有未解压 tar 包。
- `--dest DIR`：覆盖解压目录（默认为 tar 所在目录）。
- `--yes` / `-y`：自动确认交互选择。
- `--strict`：严格检测 tar 是否已被解压（打开 tar 并检查顶层成员，较慢）。
- `--summarize`：为任务生成 `info.json`。
- `--extract-crashes`：提取并移动 crash 相关文件到 `crashes/`。
- `--aggregate`：聚合多个 task 的 `info.json` 并生成 `aggregated_info.json`。
- `--plot`：为每个 subtask 生成绘图并保存到 `subtask/plot/`（默认启用注释，标注 top-5）。

绘图工具（`plot_fuzzer_stats.py`）

- 可独立使用：
   ```bash
   python3 experiments/analysis/plot_fuzzer_stats.py <path/to/fuzzer_stats.json> --outdir plots
   ```
- 可选参数：
   - `--outdir`：输出目录（默认 `plots`）。
   - `--markersize`：点标记大小（默认 2.0）。
   - `--annotate` / `--no-annotate`：启用/禁用注释（默认启用）。
   - `--annotate-top-n`：标注的 top-N（默认 5）。
   - `--dpi`：PNG DPI（默认 150）。

注意事项与已知限制

- 聚合假设任务同构；若某些 task 缺少子任务，会基于现有样本数量进行平均。
- crash 提取为移动操作（非复制）。如需保留原文件请修改实现或备份数据。
- metadata 文件名约定用于识别 hash（例如 `.H_1.metadata` 或 `H_1.metadata`）。
- `--strict` 模式会打开 tar 进行检查，处理大量 tar 时较慢。

改进建议

- 将绘图参数暴露给 `analysis.py`（例如 `--plot-dpi`/`--plot-markersize`/`--plot-annotate-top`）。
- 添加 `--dry-run` 选项用于在不修改文件的情况下预览将要提取或绘图的内容。
- 对 summarize/plot 步骤做并行化以提升大规模处理性能（需控制 I/O 并发）。

----

如需我把 README 内容调整为中文更短版、加入示例输出或把 `analysis.py` 的 CLI 扩展为更多绘图参数，我可以继续修改。 