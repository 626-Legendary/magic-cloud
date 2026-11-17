#!/bin/bash
set -euo pipefail
# Deliberately never source .env: it is data, not shell code.
project=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
config=${MC_CONFIG:-"$project/.env"}
run_dir=${MC_RUN_DIR:-/run}
value() {
  local key=$1 result
  result=$(sed -n "s/^${key}=//p" "$config")
  [[ -n "$result" && "$result" != *$'\n'* ]] || { echo "Missing or duplicate configuration: $key" >&2; exit 1; }
  [[ "$result" != \"* ]] || { echo 'Use plain unquoted values in deployment .env' >&2; exit 1; }
  printf '%s' "$result"
}
load_disk() {
  mount_path=$(value STORAGE_MOUNT)
  disk_uuid=$(value STORAGE_UUID)
  [[ "$mount_path" =~ ^/[a-zA-Z0-9_/-]+$ && "$mount_path" != / && "$disk_uuid" =~ ^[a-fA-F0-9-]+$ ]] || { echo 'Invalid mount path or UUID' >&2; exit 1; }
}
check_mount() {
  mountpoint -q -- "$mount_path" || return 1
  [[ "$(findmnt -rn -M "$mount_path" -o UUID)" == "$disk_uuid" ]] || return 1
  [[ "$(findmnt -rn -M "$mount_path" -o FSTYPE)" == ext4 ]] || return 1
  [[ ",$(findmnt -rn -M "$mount_path" -o OPTIONS)," == *,rw,* ]] || return 1
  [[ "$(readlink -f -- "$mount_path")" == "$mount_path" ]] || return 1
}
check_identity() {
  check_mount || return 1
  local expected
  expected=$(value STORAGE_ID)
  [[ -f "$mount_path/.magic-cloud/storage-id" && ! -L "$mount_path/.magic-cloud" && ! -L "$mount_path/.magic-cloud/storage-id" ]] || return 1
  [[ "$(cat -- "$mount_path/.magic-cloud/storage-id")" == "$expected" ]] || return 1
  local d
  for d in files .magic-cloud/database .magic-cloud/uploads .magic-cloud/trash .magic-cloud/thumbnails .magic-cloud/backups .magic-cloud/operations; do
    [[ -d "$mount_path/$d" && ! -L "$mount_path/$d" ]] || return 1
  done
  # Probe the USB, never the underlying mountpoint directory.
  local probe
  probe=$(mktemp "$mount_path/.magic-cloud/.probe.XXXXXXXX") || return 1
  printf 'ok' > "$probe" && sync -f "$probe" && rm -- "$probe"
}
compose() { docker compose --project-directory "$project" --env-file "$config" "$@"; }
load_disk
case "${1:-check}" in
 mount)
   if ! mountpoint -q -- "$mount_path"; then
     [[ -b "/dev/disk/by-uuid/$disk_uuid" ]] || { echo 'Expected USB is absent' >&2; exit 1; }
     mkdir -p -- "$mount_path"
     mount -t ext4 -o nodev,nosuid,noexec "UUID=$disk_uuid" "$mount_path"
   fi
   check_mount || { echo 'Mount identity mismatch' >&2; exit 1; }
   ;;
 init)
   [[ "${2:-}" == --initialize-empty ]] || { echo 'Explicit --initialize-empty is required' >&2; exit 1; }
   check_mount || { echo 'Expected ext4 USB is not mounted' >&2; exit 1; }
   [[ ! -e "$mount_path/.magic-cloud" && ! -L "$mount_path/.magic-cloud" && ! -e "$mount_path/files" && ! -L "$mount_path/files" ]] || { echo 'Existing storage: initialization refused' >&2; exit 1; }
   app_uid=$(value APP_UID); app_gid=$(value APP_GID)
   [[ "$app_uid" =~ ^[0-9]+$ && "$app_gid" =~ ^[0-9]+$ && "$app_uid" -gt 0 && "$app_gid" -gt 0 ]] || exit 1
   install -d -m 700 -o "$app_uid" -g "$app_gid" "$mount_path/files" "$mount_path/.magic-cloud"
   for d in database uploads trash thumbnails backups operations; do install -d -m 700 -o "$app_uid" -g "$app_gid" "$mount_path/.magic-cloud/$d"; done
   cat /proc/sys/kernel/random/uuid > "$mount_path/.magic-cloud/storage-id"
   chown "$app_uid:$app_gid" "$mount_path/.magic-cloud/storage-id"
   chmod 600 "$mount_path/.magic-cloud/storage-id"
   sync -f "$mount_path/.magic-cloud/storage-id"
   echo 'Initialization complete. Set STORAGE_ID in .env to:'
   cat -- "$mount_path/.magic-cloud/storage-id"
   ;;
 check)
   check_identity || { echo 'Storage is not ready; refusing backend startup' >&2; exit 1; }
   echo 'Expected USB identity and writable ext4 mount verified'
   ;;
 reconcile)
   exec 9>"$run_dir/lock/magic-cloud-recovery.lock"
   flock -n 9 || exit 0
   if ! check_identity; then
     rm -f "$run_dir/magic-cloud.last-start"
     # Stop backend before attempting a different bind mount.
     compose stop -t 5 magic-cloud >/dev/null
     if [[ -b "/dev/disk/by-uuid/$disk_uuid" ]]; then
       if mountpoint -q -- "$mount_path"; then
         current_mount_id=$(findmnt -rn -M "$mount_path" -o ID)
         previously_verified=false
         if [[ -f "$run_dir/magic-cloud.verified-mount" && "$(cat "$run_dir/magic-cloud.verified-mount")" == "$disk_uuid:$current_mount_id" ]]; then previously_verified=true; fi
         if [[ "$(findmnt -rn -M "$mount_path" -o UUID)" == "$disk_uuid" || "$previously_verified" == true ]]; then
           umount -- "$mount_path" || { echo 'Cannot detach stale USB mount' >&2; exit 1; }
         fi
       fi
       if ! mountpoint -q -- "$mount_path"; then "$0" mount >/dev/null; fi
     fi
     check_identity || { echo 'Storage unavailable; backend remains stopped' >&2; exit 1; }
   fi
   cid=$(compose ps -a -q magic-cloud)
   state=missing
   [[ -z "$cid" ]] || state=$(docker inspect -f '{{.State.Status}}' "$cid")
   mount_id=$(findmnt -rn -M "$mount_path" -o ID)
   printf '%s' "$disk_uuid:$mount_id" > "$run_dir/magic-cloud.verified-mount"
   signature=$(findmnt -rn -M "$mount_path" -o ID,MAJ:MIN)
   if [[ "$state" != running ]]; then
     if [[ -f "$run_dir/magic-cloud.last-start" && "$(cat "$run_dir/magic-cloud.last-start")" == "$signature" ]]; then
       echo 'Backend exited on this mount; inspect logs, then remove the last-start latch to retry' >&2
       exit 1
     fi
     printf '%s' "$signature" > "$run_dir/magic-cloud.last-start"
     # Recreate, never restart a container that may retain an obsolete mount.
     compose up -d --no-deps --force-recreate magic-cloud
   else
     health=$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{end}}' "$cid")
     if [[ "$health" == unhealthy ]]; then
       compose stop -t 5 magic-cloud >/dev/null
       echo 'Unhealthy backend stopped. Inspect database/storage before recovery.' >&2
       exit 1
     fi
   fi
   ;;
 *) echo 'Usage: storage.sh mount|init --initialize-empty|check|reconcile' >&2; exit 1 ;;
esac
