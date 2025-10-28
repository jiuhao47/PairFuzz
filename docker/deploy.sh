#!/usr/bin/env bash
set -euo pipefail

# Unified deploy script for diffray: supports multiple Dockerfile targets (runtime, compile, default)
# Usage: ./deploy.sh [--target runtime|compile|default] <command> [args]

TARGET=${1:-runtime}
if [ "$TARGET" = "--target" ]; then
  TARGET=${2:-runtime}
  shift 2
fi

function usage() {
  cat <<'SHORT'
Usage: deploy.sh [--target runtime|compile|default] <command> [args]

Targets:
  runtime   -> docker/runtime/Dockerfile
  compile   -> docker/compile/Dockerfile
  default   -> docker/Dockerfile (falls back to runtime/compile)

Commands: build, save, scp, load, run, add-key, stop, backup, restore, ssh
Run 'deploy.sh help' for full usage and examples.
SHORT
}

function full_usage() {
  sed -n '1,200p' "$0"
}

command=${1:-}
shift || true

select_dockerfile() {
  # choose Dockerfile according to TARGET
  case "$TARGET" in
    runtime)
      if [ -f docker/runtime/Dockerfile ]; then
        echo docker/runtime/Dockerfile
        return 0
      fi
      ;;
    compile)
      if [ -f docker/compile/Dockerfile ]; then
        echo docker/compile/Dockerfile
        return 0
      fi
      ;;
    default)
      if [ -f docker/Dockerfile ]; then
        echo docker/Dockerfile
        return 0
      fi
      if [ -f docker/runtime/Dockerfile ]; then
        echo docker/runtime/Dockerfile
        return 0
      fi
      if [ -f docker/compile/Dockerfile ]; then
        echo docker/compile/Dockerfile
        return 0
      fi
      ;;
    *)
      # try explicit path if user passed one
      if [ -f "$TARGET" ]; then
        echo "$TARGET"
        return 0
      fi
      ;;
  esac
  return 1
}

case "$command" in
  help|--help)
    full_usage
    exit 0
    ;;
  build)
    tag=${1:-diffray:latest}
    echo "Building image $tag (target=$TARGET)"
    DF=$(select_dockerfile) || {
      echo "Cannot find Dockerfile for target '$TARGET' (tried common locations)" >&2
      exit 1
    }
    echo "Using Dockerfile: $DF"
    docker build -t "$tag" -f "$DF" .
    ;;

  save)
    tag=${1:?need tag}
    out=${2:-./${tag//[:\/]/_}.tar.gz}
    echo "Saving $tag -> $out (compressed with gzip)"
    # If the output filename ends with .gz (our default), stream docker save through gzip
    if [ "${out##*.}" = "gz" ]; then
      docker save "$tag" | gzip -c > "$out"
    else
      # user explicitly requested a non-gz output filename; fall back to docker save -o
      docker save -o "$out" "$tag"
    fi
    ;;

  scp)
    tarfile=${1:?tarfile}
    dest=${2:?user@host:path}
    echo "Copying $tarfile to $dest (gzip-compressed transfer)"
    # If the file is already gzipped, copy directly. Otherwise create a temporary gz and transfer it.
    if [[ "$tarfile" == *.gz ]]; then
      scp "$tarfile" "$dest"
    else
      tmp=$(mktemp --suffix=.gz)
      gzip -c "$tarfile" > "$tmp"
      scp "$tmp" "$dest"
      rm -f "$tmp"
    fi
    ;;

  load)
    remote_tar=${1:?user@host:/path/to/tar}
    echo "Loading image on remote host: $remote_tar"
    host="${remote_tar%%:*}"
    path="${remote_tar#*:}"
    # If the remote file is gzipped, use gunzip -c to stream into docker load on the remote host.
    if [[ "$path" == *.gz ]]; then
      ssh "$host" "gunzip -c '$path' | docker load"
    else
      ssh "$host" "docker load -i '$path'"
    fi
    ;;

  run)
    host=${1:?user@host}
    shift
    PORT=2222
    NAME=diffray-service
    VOLUME_ARGS=()
    ENV_ARGS=()
    PASS_HOST_UID=1
    IMAGE=${1:-diffray:latest}
    TARGET_VOL=target_switch
    RUN_VOL=diffray_runs
    CARGO_VOL=cargo_cache
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --port) PORT=$2; shift 2;;
        --name) NAME=$2; shift 2;;
        --volume) VOLUME_ARGS+=("-v" "$2"); shift 2;;
        --image) IMAGE=$2; shift 2;;
        --env) ENV_ARGS+=("$2"); shift 2;;
        --pass-host-uid) PASS_HOST_UID=1; shift;;
        --no-pass-host-uid) PASS_HOST_UID=0; shift;;
        --target-volume) TARGET_VOL=$2; shift 2;;
        --run-volume) RUN_VOL=$2; shift 2;;
        --cargo-volume) CARGO_VOL=$2; shift 2;;
        *) shift;;
      esac
    done

    echo "Running on $host: image=$IMAGE name=$NAME host_port=$PORT"
    ssh "$host" "docker volume inspect ${TARGET_VOL} >/dev/null 2>&1 || docker volume create ${TARGET_VOL} || true"
    ssh "$host" "docker volume inspect ${RUN_VOL} >/dev/null 2>&1 || docker volume create ${RUN_VOL} || true"
    ssh "$host" "docker volume inspect ${CARGO_VOL} >/dev/null 2>&1 || docker volume create ${CARGO_VOL} || true"

    VOL_STR="-v ${TARGET_VOL}:/workspace/target -v ${RUN_VOL}:/workspace/runs -v ${CARGO_VOL}:/root/.cargo"
    if [ ${#VOLUME_ARGS[@]} -gt 0 ]; then
      for ((i=0;i<${#VOLUME_ARGS[@]};i+=2)); do
        arg_path=${VOLUME_ARGS[i+1]}
        esc_path=$(printf "%s" "$arg_path" | sed "s/'/'\\''/g")
        VOL_STR="$VOL_STR -v '$esc_path'"
      done
    fi

    REMOTE_CMD="docker rm -f $NAME >/dev/null 2>&1 || true && docker run -d --name $NAME --restart unless-stopped -p ${PORT}:2222 ${VOL_STR} --env SSH_PORT=2222 --env START_SSHD=1 --privileged=false --user root ${IMAGE}"

    ENV_STR=""
    if [ ${#ENV_ARGS[@]} -gt 0 ]; then
      for envpair in "${ENV_ARGS[@]}"; do
        esc=$(printf "%s" "$envpair" | sed "s/'/'\\''/g")
        ENV_STR="$ENV_STR --env '$esc'"
      done
    fi

    if [ "$PASS_HOST_UID" -eq 1 ]; then
      LOCAL_UID=$(id -u)
      LOCAL_GID=$(id -g)
      ENV_STR="$ENV_STR --env HOST_UID=$LOCAL_UID --env HOST_GID=$LOCAL_GID"
    fi

    REMOTE_CMD="docker rm -f $NAME >/dev/null 2>&1 || true && docker run -d --name $NAME --restart unless-stopped -p ${PORT}:2222 ${VOL_STR} ${ENV_STR} --env SSH_PORT=2222 --env START_SSHD=1 --privileged=false --user root ${IMAGE}"

    ssh "$host" "$REMOTE_CMD"
    echo "Started container $NAME on $host:$PORT"
    ;;

  ssh)
    host=${1:?user@host}
    port=${2:-2222}
    name=${3:-diffray-service}
    echo "SSH to container $name on $host (ssh on host port $port)"
    echo "To get interactive shell via docker exec, run on host: docker exec -it $name /bin/bash"
    ;;

  stop)
    host=${1:?user@host}
    name=${2:-diffray-service}
    echo "Stopping container $name on $host"
    ssh "$host" "docker rm -f $name || true"
    ;;

  add-key)
    host=${1:?user@host}
    name=${2:?container}
    pubkey=${3:?local_pubkey}
    echo "Adding key $pubkey to $name on $host"
    ssh "$host" "docker exec -i $name /bin/sh -c 'mkdir -p /root/.ssh && cat >> /root/.ssh/authorized_keys'" < "$pubkey"
    ssh "$host" "docker exec -i $name /bin/sh -c 'chown -R root:root /root/.ssh && chmod 700 /root/.ssh && chmod 600 /root/.ssh/authorized_keys'"
    ;;

  backup)
    host=${1:?user@host}
    vol=${2:?volume}
    out=${3:-/tmp/${vol}.tgz}
    echo "Backing up volume $vol on $host to $out"
    ssh "$host" "docker run --rm -v ${vol}:/data -v /tmp:/backup busybox sh -c 'cd /data && tar czf /backup/${vol}.tgz .'; mv /tmp/${vol}.tgz ${out}"
    ;;

  restore)
    host=${1:?user@host}
    vol=${2:?volume}
    inpath=${3:?remote_path_to_tgz}
    echo "Restoring $inpath into $vol on $host"
    ssh "$host" "docker run --rm -v ${vol}:/data -v /tmp:/backup busybox sh -c 'cd /data && tar xzf /backup/$(basename ${inpath})'"
    ;;

  *)
    usage
    exit 1
    ;;
esac
