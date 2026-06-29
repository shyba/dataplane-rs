#!/usr/bin/env bash

dp_default_log_root() {
  local env_name="$1"
  local value="${!env_name:-/home/user/mnt/dataplane/logs}"
  printf '%s' "$value"
}

dp_new_run_id() {
  printf '%s-%s' "$(date -u +%Y%m%dT%H%M%SZ)" "$$"
}

dp_artifact_path() {
  local log_root="$1"
  local prefix="$2"
  local run_id="$3"
  local suffix="$4"
  printf '%s/%s-%s.%s' "$log_root" "$prefix" "$run_id" "$suffix"
}

dp_emit_artifact_line() {
  local label="$1"
  local path="$2"
  printf '%s: %s\n' "$label" "$path"
}

dp_extract_artifact() {
  local log_file="$1"
  local label="$2"
  local line path candidate matches=0
  if [[ ! -f "$log_file" ]]; then
    return 1
  fi
  line=""
  while IFS= read -r candidate; do
    case "$candidate" in
      "$label":\ /*)
        line="$candidate"
        matches=$((matches + 1))
        ;;
    esac
  done <"$log_file" || return 1
  if [[ "$matches" -ne 1 ]]; then
    return 1
  fi
  path="${line#"$label": }"
  if [[ -z "$path" ]]; then
    return 1
  fi
  if [[ "$path" != /home/user/mnt/dataplane/* ]]; then
    return 1
  fi
  if [[ ! -s "$path" ]]; then
    return 1
  fi
  printf '%s' "$path"
}

dp_require_marker() {
  local log_file="$1"
  local marker="$2"
  local status
  set +e
  grep -Fq "$marker" "$log_file"
  status=$?
  set -e
  [[ "$status" -eq 0 ]]
}

dp_require_key_value() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local line candidate
  if [[ ! -f "$file" ]]; then
    return 1
  fi
  line=""
  while IFS= read -r candidate; do
    case "$candidate" in
      "$key"=*) line="$candidate" ;;
    esac
  done <"$file" || return 1
  if [[ -z "$line" ]]; then
    return 1
  fi
  [[ "$line" == "$key=$expected" ]]
}

dp_require_summary_run_id() {
  local file="$1"
  local expected="$2"
  dp_require_key_value "$file" "validation_matrix_run_id" "$expected"
}

dp_reject_duplicate_keys() {
  local file="$1"
  local keys
  if [[ ! -f "$file" ]]; then
    return 1
  fi
  keys="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)" || return 1
  [[ -z "$keys" ]]
}
