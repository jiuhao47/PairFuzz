# 第三方组件与许可证

本文是参赛评审使用的人类可读索引。精确 URL、tag、commit、归档或文件 SHA-256 和
许可证字段由以下机器可读文件固定：

- `third_party/json-libs.json`：仓库内保存的 cJSON 与 mjson 最小源码字节；
- `third_party/real-targets.lock.json`：按需下载的原生目标、zlib 和 XXH64；
- `Cargo.lock`、`app/compiler/Cargo.lock`：Rust 直接与传递依赖；
- `LibAFL` gitlink：固定上游源码提交。

第三方许可证只约束相应组件。PairFuzz 原创代码采用 MIT 许可证，全文见 [LICENSE](LICENSE)；
该授权不改变 LibAFL、目标库或其他第三方组件各自的许可证和分发义务。

## 运行框架

| 组件 | 固定版本 | 用途 | 许可证与原文 |
| --- | --- | --- | --- |
| LibAFL | 0.16.1，gitlink `92ad89454765440c7c03b87b11d5dbde753f9633` | 执行器、forkserver、语料、调度、变异和事件管理 | MIT 或 Apache-2.0；`LibAFL/LICENSE-MIT`、`LibAFL/LICENSE-APACHE` |

其他 Rust crate 的作者、版本和许可证应结合锁文件与对应 crate 元数据核对；锁文件是
本仓库实际解析版本的依据。

## JSON 最小用例

| 组件 | 固定版本 | 仓库方式 | 许可证 |
| --- | --- | --- | --- |
| cJSON | 1.7.19 / `v1.7.19` | 保存构建所需最小源码和许可证，逐文件哈希固定 | MIT |
| mjson | 1.2.7 / `1.2.7` | 保存构建所需最小源码和许可证，逐文件哈希固定 | MIT |

可运行 `python3 scripts/real_json_demo.py verify-vendor` 验证这些字节。cJSON 1.7.19 的
JSON Patch 递归耗尽通告涉及未参与本演示构建的可选 `cJSON_Utils`；边界记录在
`docs/contest/provenance.md`。

## 原生语义差分目标

| 组件 | 固定版本 | 操作 | 许可证 |
| --- | --- | --- | --- |
| libpng | 1.6.59 / `v1.6.59` | PNG→RGBA8 | libpng-2.0 |
| libspng | 0.7.4 / `v0.7.4` | PNG→RGBA8 | BSD-2-Clause |
| ImageMagick | 7.1.2-34 | BMP→RGBA8 | Apache-2.0 |
| GraphicsMagick | 1.3.48 | BMP→RGBA8 | MIT |
| OpenSSL | 4.1.0 / `openssl-4.1.0` | DER X.509 解析与重编码 | Apache-2.0 |
| wolfSSL | 5.9.4-stable / `v5.9.4-stable` | DER X.509 解析与重编码 | GPL-3.0-or-later |
| zlib | 1.3.1 / `v1.3.1` | PNG 压缩依赖 | Zlib |
| XXH64（xxHash） | 0.8.3 / `v0.8.3` | 规范化结果的 64 位热路径指纹 | BSD-2-Clause |

仓库不提交这些大型上游构建树或链接二进制；它们位于被忽略的
`run/real-targets/`。如果对外分发链接 wolfSSL 的目标二进制，必须单独履行
GPL-3.0-or-later 的对应源码与许可证义务。仅提交适配器、契约和小型 fixture 不会消除
分发产物时的义务。

## 归属边界

PairFuzz 原创或历史项目代码主要位于 `src/`、`app/compiler/`、目标适配器和工作流脚本；
LibAFL 以及上述解析/图像/密码库仍归各自上游作者所有。历史恢复点、本次新增贡献和
借鉴范围见 [代码来源与依赖归属](docs/contest/provenance.md)。
