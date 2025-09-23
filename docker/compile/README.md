# diffray — runtime Docker 镜像说明

本目录包含用于运行 diffray runtime 的 Docker 相关说明与示例。本 README 提供构建、运行、调试和常见问题的快速指南，帮助在容器中启动并调试 runtime 环境。

## 目标

- 提供一个可重现的、隔离的运行环境，用以执行 fuzzing / 比较 / 测试程序（runtime）。
- 便于挂载宿主机的输入/输出目录与调试工具，快速进行本地验证。

## 目录结构（概览）

- Dockerfile (如果存在)：用于构建 runtime 镜像的定义。
- 辅助脚本（如有）：启动容器或帮助调试的 shell 脚本。
- 本 README：使用说明与示例。

> 注：如果本目录缺少 Dockerfile，请参考仓库根目录的 `docker/` 子目录或联系维护者获取构建文件。

## 要求

- Docker（建议版本 20.x+）或兼容的容器运行时。
- 如果要在容器中运行带 GPU/特权的程序，请确保宿主机已正确配置相应驱动与运行时（不在本 README 详细说明）。
## 常见运行场景

  -v $(pwd):/workspace \
```bash
- 若 CI runner 已有 Docker-in-Docker 功能，可直接在 pipeline 中构建并运行镜像。

# diffray runtime — deploy.sh 使用说明

本文件重点介绍本目录下的 `deploy.sh`（路径：`docker/runtime/deploy.sh`）的用途与用法。
`deploy.sh` 是一个方便的运维脚本，用于：构建镜像、保存为 tar、通过 scp 传输到远端、在远端加载镜像并运行容器、以及备份/恢复数据卷等。

在使用前请确保你可以通过 SSH 免密（或密钥）访问目标主机，且目标主机已安装 Docker。

---

## 快速帮助

运行脚本主帮助：

```bash
./docker/runtime/deploy.sh help
```

脚本支持的高层命令：

- build [tag]
- save <tag> <out_tar>
- scp <tar> <user@host>:path
- load <user@host:/path/to/tar>
- run <user@host> [opts]
- ssh <user@host> [port]
- stop <user@host> <name>
- add-key <user@host> <container-name> <local-pubkey>
- backup <user@host> <vol> <out_path>
- restore <user@host> <vol> <in_path>

下面我会逐条解释并给出示例。

## 1) build — 在本地构建镜像

用途：在本地根据仓库中的 Dockerfile 构建镜像。

示例：

```bash
./docker/runtime/deploy.sh build my-diffray:latest
# 若不指定 tag，默认使用 diffray:latest
```

脚本内部会执行 `docker build -t <tag> -f docker/Dockerfile .`，请确保在仓库根目录运行或修改脚本以匹配 Dockerfile 路径。

## 2) save — 把镜像保存为 tar 文件

用途：将本地镜像打包为 tar，便于通过 scp 传输或离线导入。

示例：

```bash
./docker/runtime/deploy.sh save my-diffray:latest /tmp/my-diffray.tar
```

如果 `out_tar` 未指定，脚本会根据 tag 自动生成一个默认文件名。

## 3) scp — 把 tar 文件复制到远端

用途：通过 scp 将打包好的镜像传到远端主机。

示例：

```bash
./docker/runtime/deploy.sh scp /tmp/my-diffray.tar user@remote.host:/tmp/my-diffray.tar
```

注意：脚本本身不会自动触发远端的 `docker load`，你可以随后使用 `load` 命令或手动在远端运行 `docker load -i /tmp/my-diffray.tar`。

## 4) load — 在远端加载镜像 tar

用途：通过 SSH 在远端主机上执行 `docker load -i <tar>`，把镜像导入远端本地镜像库。

示例：

```bash
./docker/runtime/deploy.sh load user@remote.host:/tmp/my-diffray.tar
```

脚本会通过 SSH 登录到 `user@remote.host` 并执行加载命令。

## 5) run — 在远端运行容器（最常用）

用途：在远端主机创建/启动容器并映射端口、挂载数据卷。

基本用法：

```bash
./docker/runtime/deploy.sh run user@remote.host --port 2222 --name my-diffray --image my-diffray:latest --volume /host/path:/workspace
```

主要选项说明：

- `--port PORT`：把远端主机的 PORT（宿主）映射到容器内部的 2222（脚本约定容器内 sshd 监听 2222）。
- `--name NAME`：容器名称（默认 diffray-service）。
- `--volume HOSTPATH:CONTAINERPATH`：将宿主路径挂载到容器内（可多次使用）。
- `--image IMAGE`：要启动的镜像（默认 diffray:latest）。
- `--env KEY=VAL`：传递环境变量到容器（可多次使用）。
- `--pass-host-uid` / `--no-pass-host-uid`：是否把本地 UID/GID 传给容器环境变量（默认传递），用于避免文件权限问题。
- `--target-volume`、`--run-volume`、`--cargo-volume`：脚本会为这些命名卷创建或复用卷，用于持久化 target、runs、cargo 缓存；可通过选项修改名称。

脚本行为（关键点）：

- 在远端创建（若不存在）命名卷（target/run/cargo）。
- 构造 `docker run -d --name ... -p <PORT>:2222 -v ... --env START_SSHD=1 <IMAGE>` 并在远端通过 SSH 执行它。
- 默认容器以 `root` 运行以便绑定端口与写入卷（容器内服务仍可降权运行）。

示例（允许多个 volume、传 env）：

```bash
./docker/runtime/deploy.sh run user@remote.host --port 2200 --name diffray-demo --image my-diffray:latest --volume /home/user/seeds:/seeds --env RUST_LOG=info --pass-host-uid
```

执行完成后脚本会打印 `Started container <name> on <host>:<port>`。

## 6) ssh — 通过容器暴露的 SSH 进入

说明：脚本提供 `ssh` 命令描述，但真正进入容器通常有两种方式：

- 使用脚本的 `ssh`（连接到远端主机并通过映射端口访问容器内 sshd），示例：
  ```bash
  ./docker/runtime/deploy.sh ssh user@remote.host 2200
  # 然后从本地使用 ssh -p 2200 root@remote.host (或相应用户)
  ```
- 或者在远端通过 `docker exec -it <name> /bin/bash` 直接进入（脚本提示该命令）。

注：要使用第一个方式，需要在 run 时映射主机端口，并在防火墙允许该端口访问；第二种方式需要 shell/SSH 访问远端主机来执行 docker exec。

## 7) stop — 停止并移除远端容器

```bash
./docker/runtime/deploy.sh stop user@remote.host my-diffray
```

脚本会在远端执行 `docker rm -f <name>`。

## 8) add-key — 把本地公钥加入容器的 root authorized_keys

用途：把你的公钥复制到容器内 `/root/.ssh/authorized_keys`，以便用 SSH 直接登录到容器（经由远端主机映射的端口）。

示例：

```bash
./docker/runtime/deploy.sh add-key user@remote.host my-diffray /home/user/.ssh/id_rsa.pub
```

脚本会把公钥通过 SSH 上传并在容器内追加到 `/root/.ssh/authorized_keys`，并设置严格的权限。

## 9) backup / restore — 备份与恢复命名卷

用途：把远端命名卷（例如 runs、target）内容打包为 tar 到远端主机的临时目录，或从该 tar 恢复。

示例：备份卷

```bash
./docker/runtime/deploy.sh backup user@remote.host diffray_runs /tmp/diffray_runs.tgz
```

示例：恢复卷

```bash
./docker/runtime/deploy.sh restore user@remote.host diffray_runs /tmp/diffray_runs.tgz
```

脚本内部使用一个临时 BusyBox 容器来打包/解包卷内容。

## 常见问题与排查

- 无法 SSH 到远端主机：确保你能从本地无密码或使用密钥登录 `user@host`，并且 `ssh user@host docker` 等命令可以执行（docker 在远端可用）。
- `docker: command not found`（远端）：请在目标主机上安装 Docker 或联系管理员。
- 端口映射后无法访问容器 SSH：检查防火墙（iptables / cloud security group）是否允许该端口，且 run 时指定的 `--port` 值不与 host 上其他服务冲突。
- 文件权限问题：如果容器内产生的文件被 root 拥有，宿主用户无法访问，可使用 `--pass-host-uid` 选项或在 `docker run` 时传递 `-u` 进行用户映射（脚本已提供 HOST_UID/HOST_GID 环境变量传递）。

## 推荐部署流程（示例）

1. 在本地构建镜像：

```bash
./docker/runtime/deploy.sh build my-diffray:latest
```

2. 保存为 tar 并传输到远端（或直接使用私有 registry）：

```bash
./docker/runtime/deploy.sh save my-diffray:latest /tmp/my-diffray.tar
./docker/runtime/deploy.sh scp /tmp/my-diffray.tar user@remote.host:/tmp/my-diffray.tar
```

3. 在远端加载镜像并运行容器：

```bash
./docker/runtime/deploy.sh load user@remote.host:/tmp/my-diffray.tar
./docker/runtime/deploy.sh run user@remote.host --port 2222 --name diffray-service --image my-diffray:latest --pass-host-uid
```

4. 可选：把你的公钥加入容器并通过 ssh 登录：

```bash
./docker/runtime/deploy.sh add-key user@remote.host diffray-service ~/.ssh/id_rsa.pub
ssh -p 2222 root@remote.host
```

## 安全注意事项

- 默认不启用基于密码的登录，脚本推荐使用 SSH key 认证。
- `--privileged` 参数在脚本运行时设为 `false`；若你确实需要让容器拥有更多主机访问权限，请谨慎对待并仅在受信任环境中使用。

---

如果你希望我把 README 中的示例改为更贴合你们生产/测试环境（例如使用私有 registry、CI/CD 自动推送、或者把 `docker run` 的卷映射改为宿主路径映射示例），把目标主机、镜像命名或常用卷名告诉我，我会据此定制示例。
