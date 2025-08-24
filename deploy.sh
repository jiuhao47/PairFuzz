#!/usr/bin/env bash
set -euo pipefail

# deploy.sh - build, transfer and run diffray container with optional SSH access
# Usage: ./deploy.sh <command> [args]
# Commands:
#   build [tag]                 Build docker image locally. Default tag: diffray:latest
#   save <tag> <out_tar>        Save image to tar
#   scp <tar> <user@host>:path  Copy tar to remote host
#   load <remote_tar>           Load image from tar on remote host (via ssh)
#   run <user@host> [opts]      Run container on remote host; opts: --port PORT --name NAME --volume HOSTPATH:CONTAINERPATH
#   ssh <user@host> [port]      SSH into container via docker exec (requires container running and SSH exposed)
#   stop <user@host> <name>     Stop and remove container
#   backup <user@host> <vol> <out_path>  Backup named volume
#   restore <user@host> <vol> <in_path> Restore named volume from tar
#
# Notes:
# - This script assumes you have SSH key access to the remote host and docker installed there.
# - By default the container will expose SSH on port 2222 inside the container; the script maps it to a host port you provide.

IMG_TAG=${1:-}

function usage() {
  cat <<'SHORT'
Usage: deploy.sh <command> [args]

Commands: build, save, scp, load, run, add-key, stop, backup, restore, ssh
Run 'deploy.sh help' for full usage and examples.
SHORT
}

function full_usage() {
  sed -n '1,200p' "$0"
}

command=${1:-}
shift || true

case "$command" in
  help|--help)
    full_usage
    exit 0
    ;;
  build)
    tag=${1:-diffray:latest}
    echo "Building image $tag"
    sudo docker  build -t "$tag" -f docker/Dockerfile .
    ;;

  save)
    tag=${1:?need tag}
    out=${2:-./${tag//[:\/]/_}.tar}
    echo "Saving $tag -> $out"
    sudo docker  save -o "$out" "$tag"
    ;;

  scp)
    tarfile=${1:?tarfile}
    dest=${2:?user@host:path}
    echo "Copying $tarfile to $dest"
    scp "$tarfile" "$dest"
    ;;

  load)
    remote_tar=${1:?user@host:/path/to/tar}
    echo "Loading image on remote host: $remote_tar"
    ssh "${remote_tar%%:*}" "sudo docker  load -i ${remote_tar#*:}"
    ;;

  run)
    # run <user@host> [--port PORT] [--name NAME] [--volume hostpath:containerpath]
    host=${1:?user@host}
    shift
    PORT=2222
    NAME=diffray-service
    VOLUME_ARGS=()
  IMAGE=${1:-diffray:latest}
  # default volume names
  TARGET_VOL=target_switch
  RUN_VOL=diffray_runs
  CARGO_VOL=cargo_cache
  # parse options
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --port) PORT=$2; shift 2;;
        --name) NAME=$2; shift 2;;
        --volume) VOLUME_ARGS+=("-v" "$2"); shift 2;;
        --image) IMAGE=$2; shift 2;;
      --target-volume) TARGET_VOL=$2; shift 2;;
      --run-volume) RUN_VOL=$2; shift 2;;
      --cargo-volume) CARGO_VOL=$2; shift 2;;
        *) shift;;
      esac
    done

    echo "Running on $host: image=$IMAGE name=$NAME host_port=$PORT"
    # Create named volumes for persistent data if not exist
  ssh "$host" "sudo docker  volume inspect ${TARGET_VOL} >/dev/null 2>&1 || sudo docker  volume create ${TARGET_VOL} || true"
  ssh "$host" "sudo docker  volume inspect ${RUN_VOL} >/dev/null 2>&1 || sudo docker  volume create ${RUN_VOL} || true"
  ssh "$host" "sudo docker  volume inspect ${CARGO_VOL} >/dev/null 2>&1 || sudo docker  volume create ${CARGO_VOL} || true"

    # Build run command: map host port to container SSH port (env SSH_PORT)
    # We'll run container as root so sshd can bind to non-privileged port; but container still can run services as non-root inside
    REMOTE_CMD=$(cat <<EOF
sudo docker  rm -f $NAME >/dev/null 2>&1 || true && \
 sudo docker  run -d --name $NAME \
  --restart unless-stopped \
  -p ${PORT}:2222 \
      -v ${TARGET_VOL}:/home/dev/target \
      -v ${RUN_VOL}:/home/dev/runs \
      -v ${CARGO_VOL}:/home/dev/.cargo \
  ${VOLUME_ARGS[@]} \
  --env SSH_PORT=2222 \
  --privileged=false \
  --user root \
  ${IMAGE} /usr/sbin/sshd-start.sh
EOF
)
    ssh "$host" "$REMOTE_CMD"
    echo "Started container $NAME on $host:$PORT"
    ;;

  ssh)
    # ssh into container via host: uses ssh-key to access host then docker exec into container
    host=${1:?user@host}
    port=${2:-2222}
    name=${3:-diffray-service}
    echo "SSH to container $name on $host (ssh on host port $port)"
    # We use SSH to connect to the host's mapped port which forwards to container's sshd
    # First ensure host allows that port from outside; if you want to use docker exec instead:
    echo "To get interactive shell via docker exec, run on host: docker exec -it $name /bin/bash"
    ;;

  stop)
    host=${1:?user@host}
    name=${2:-diffray-service}
    echo "Stopping container $name on $host"
    ssh "$host" "sudo docker  rm -f $name || true"
    ;;

  add-key)
    # add-key <user@host> <container-name> <local-pubkey-file>
    host=${1:?user@host}
    name=${2:?container}
    pubkey=${3:?local_pubkey}
    echo "Adding key $pubkey to $name on $host"
    keydata=$(cat "$pubkey" | sed 's/\$/\\$/g')
    ssh "$host" "sudo docker  exec -i $name /bin/sh -c 'mkdir -p /home/dev/.ssh && cat >> /home/dev/.ssh/authorized_keys'" < "$pubkey"
    ssh "$host" "sudo docker  exec -i $name /bin/sh -c 'chown -R dev:dev /home/dev/.ssh && chmod 700 /home/dev/.ssh && chmod 600 /home/dev/.ssh/authorized_keys'"
    ;;

  # password management removed: password login is intentionally unsupported

  backup)
    host=${1:?user@host}
    vol=${2:?volume}
    out=${3:-/tmp/${vol}.tgz}
    echo "Backing up volume $vol on $host to $out"
    ssh "$host" "sudo docker  run --rm -v ${vol}:/data -v /tmp:/backup busybox sh -c 'cd /data && tar czf /backup/${vol}.tgz .'; mv /tmp/${vol}.tgz ${out}"
    ;;

  restore)
    host=${1:?user@host}
    vol=${2:?volume}
    inpath=${3:?remote_path_to_tgz}
    echo "Restoring $inpath into $vol on $host"
    ssh "$host" "sudo docker  run --rm -v ${vol}:/data -v /tmp:/backup busybox sh -c 'cd /data && tar xzf /backup/$(basename ${inpath})'"
    ;;

  *)
    usage
    exit 1
    ;;
esac
