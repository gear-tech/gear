#!/usr/bin/env bash

echo "Disabling address space randomization"
echo 0 > /proc/sys/kernel/randomize_va_space
echo

echo "Disabling frequency scaling"
for i in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
do
  echo performance > "/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_governor"
done
echo

echo "Disabling SMT"
echo off | sudo tee /sys/devices/system/cpu/smt/control
echo "Warning: To enable SMT back it's advised to reboot system"
echo

echo "Disabling frequency boost"
if [ -f /sys/devices/system/cpu/intel_pstate ]; then
  # Intel
  echo 1 > /sys/devices/system/cpu/intel_pstate/no_turbo
else
  # AMD
  echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost
fi
echo
