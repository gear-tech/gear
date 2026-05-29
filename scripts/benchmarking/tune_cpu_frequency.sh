#!/usr/bin/env bash

# Best-effort CPU frequency tuning for benchmark runners.
#
# On real bare-metal Linux this can lock the governor/frequency. On KVM guests
# the host usually owns P-states, so this script logs the unavailable controls
# instead of pretending the frequency was fixed.

set -u

TARGET_MAX_FREQ_KHZ="${BENCHMARK_CPU_MAX_FREQ_KHZ:-3000000}"
STRICT="${BENCHMARK_CPU_FREQ_STRICT:-false}"
changed=false
failed=false

warn() {
  echo "[!] $*"
}

print_cpu_summary() {
  if command -v lscpu >/dev/null 2>&1; then
    lscpu | grep -E 'Model name|CPU\(s\)|Thread|Core|Socket|MHz|BogoMIPS' || true
  else
    warn "lscpu is not installed"
  fi
}

try() {
  echo "[+] $*"
  if "$@"; then
    changed=true
  else
    warn "Command failed: $*"
    failed=true
  fi
}

echo "[+] CPU frequency target: ${TARGET_MAX_FREQ_KHZ} kHz"
print_cpu_summary

if command -v cpupower >/dev/null 2>&1; then
  try sudo cpupower frequency-set --governor performance
  try sudo cpupower frequency-set --max "${TARGET_MAX_FREQ_KHZ}"
  sudo cpupower frequency-info || true
else
  warn "cpupower is not installed"
  failed=true
fi

policy_found=false
for policy in /sys/devices/system/cpu/cpufreq/policy*; do
  [ -d "$policy" ] || continue
  policy_found=true

  echo "[+] Inspecting ${policy}"
  for file in cpuinfo_min_freq cpuinfo_max_freq scaling_min_freq scaling_max_freq scaling_cur_freq scaling_governor; do
    [ -r "${policy}/${file}" ] && echo "    ${file}: $(cat "${policy}/${file}")"
  done

  if [ -e "${policy}/scaling_governor" ]; then
    if echo performance | sudo tee "${policy}/scaling_governor" >/dev/null; then
      changed=true
    else
      warn "Could not set ${policy}/scaling_governor"
      failed=true
    fi
  fi

  if [ -e "${policy}/scaling_max_freq" ]; then
    if echo "${TARGET_MAX_FREQ_KHZ}" | sudo tee "${policy}/scaling_max_freq" >/dev/null; then
      changed=true
    else
      warn "Could not set ${policy}/scaling_max_freq"
      failed=true
    fi
  fi
done

if [ "$policy_found" = false ]; then
  warn "No /sys/devices/system/cpu/cpufreq policies are visible in this guest"
  failed=true
fi

echo "[+] CPU frequency state after tuning attempt"
print_cpu_summary

if [ "$changed" = true ]; then
  echo "[+] CPU frequency tuning command(s) succeeded"
else
  warn "No CPU frequency tuning command succeeded"
fi

if [ "$STRICT" = true ] && [ "$failed" = true ]; then
  echo "[-] CPU frequency tuning failed in strict mode"
  exit 1
fi
