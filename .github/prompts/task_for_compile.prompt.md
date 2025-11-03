---
mode: agent
---
请按照下面的模板任务执行实现工作。注意：文中占位符 <LIB> 表示库名（例如 xz、bzip2、png 等）；对话中我会告诉你替换为哪个库。

---
兼要点（High-level goals）

- 首要步骤：在仓库内定位目标库的源码/实现目录（常见目录：`app/`、`libs/`、`build/`、`harness/`、`differential_libs/`、`LocateFix/` 等）。在实现前，把定位到的源码路径（单行路径）写入交付物，并说明选择该实现的理由；如果存在多个实现/变体，列出并说明选择依据。

- 行为对等性要求：实现必须参照 `app/script/<LIB>/compile_commit_hash.sh` 的构建步骤（环境变量、configure/compile/link flags、补丁应用、替换逻辑等），将这些关键命令逐项映射到 `LocateFix/src/scripts/<LIB>/` 下的实现中。若无法完全等价，交付物中需对每一项差异给出说明与理由。
   - 其中CC，CXX，CFLAGS，CXXFLGASS，COMMON_ARGS，JOBS需使用基类中的方法实现获取。

- 依赖复现：若 `compile_commit_hash.sh` 依赖仓库中其它资源（补丁、外部脚本、特定工具链、交叉编译设置等），实现要么复现这些依赖要么在交付物中列出无法复现项并给出替代方案。

可执行清单（Checklist）

1) 参照实现与位置
   - 参考实现示例：`LocateFix/src/scripts/libpng/libpng_compiler.py`。
   - 将等效逻辑实现到 `LocateFix/src/scripts/<LIB>/`（脚本或 Python 模块），保持与仓库惯例一致。
   - 在实现前，必须在仓库内搜索并确认目标库源码位置；若多个实现并存，列出所有候选并说明最终选取的理由。

2) 测试源文件
   - 为构建/链接验证创建最小测试程序 `test_<LIB>.c` 或 `test_<LIB>.cc`（参考 `app/libs/build_png_libs/test_libpng.cc`）。
   - 测试文件应包含可编译的 main 并引用少量符号，源码注释请使用英文。

3) 路径與变量说明
   - `<LIB>`：占位符，替换为具体库名（例如 `xz`、`bzip2`、`png`）。
   - 常用参考路径：
     - 示例脚本：`app/script/xz/compile_commit_hash.sh`
     - 参考实现：`LocateFix/src/scripts/libpng/libpng_compiler.py`
     - 目标实现目录：`LocateFix/src/scripts/<LIB>/<LIB>_compiler.py`
     - 测试源示例：`app/libs/build_png_libs/test_libpng.cc`
     - 目标测试源：`app/libs/build_<LIB>_libs/test_<LIB>.c` 或 `test_<LIB>.cc`

4) 编码与仓库约束
   - 所有源代码内的注释必须为英文（仓库强制要求）。
   - 绝对不要修改 `LibAFL/` 目录下的文件（可读取参考但不可改写）。

5) 验收标准（What success looks like）
   - 在 `LocateFix/src/scripts/<LIB>/` 提供实现（脚本或 Python 模块），并在交付物中列出仓库中定位到的源码路径（单行）。
   - 实现行为应能按 `app/script/<LIB>/compile_commit_hash.sh` 的逻辑执行对应构建/测试，或在无法复现时逐项列出差异并说明原因。
   - 为验证提供的 `test_<LIB>.c`/`test_<LIB>.cc` 能本地编译。
   - 若修改了 Rust 代码，必须先运行并通过 `cargo check --all-targets --workspace`，并把 `cargo check` 的输出（ERROR/WARNING/NOTE）一并提交。
   - 交付物必须包含：
     - 仓库中定位到的源码路径（单行路径）。
     - 可复制执行的完整验证命令行。
     - `app/script/<LIB>/compile_commit_hash.sh` 中关键步骤与 `LocateFix` 实现对应处的逐项对照（至少包含 configure/make/cc/link 和任何补丁说明）。

6) 交付物清单与说明要求
   - 列出所有新增/修改文件（相对路径），每个文件给出 1-2 行用途说明。
   - 简要列出实现要点与潜在风险（如依赖版本、工具链差异）。
   - 提供最小的本地验证命令集（如何编译并运行 `test_<LIB>`）。

补充说明

- 若需新增依赖，请先在交付物中说明原因及对构建时间/依赖树的影响，并获得人工确认后再添加。
- 若任务看似需要修改 `LibAFL/`，必须先提供替代方案并取得明确授权，切勿直接修改。

示例（将 `<LIB>` 替换为 `xz`）

- 在 `LocateFix/src/scripts/xz/` 中实现 `xz_compiler.py`（或等效脚本），并在同目录下添加 `test_xz.cc`，行为等价于 `app/script/xz/compile_commit_hash.sh`。

---
在对话中我会给出具体要替换的库名（例如 `xz` 或 `bzip2`）；请以我提供的库名替换 `<LIB>` 并完成实现。