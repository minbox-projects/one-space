#!/bin/sh
# Fixed, read-only Linux collector. Every value is read from kernel interfaces;
# no user input is interpolated into this program text. Sections are framed so
# Rust can parse each source independently and degrade only what is missing.

emit() {
    printf '@@ONESPACE:%s@@\n' "$1"
}

platform=$(uname -s 2>/dev/null || true)
emit platform
printf '%s\n' "$platform"

if [ "$platform" != "Linux" ]; then
    exit 1
fi

emit boot_id_1
cat /proc/sys/kernel/random/boot_id 2>/dev/null || true
emit uptime_1
cat /proc/uptime 2>/dev/null || true
emit stat_1
cat /proc/stat 2>/dev/null || true
emit net_1
cat /proc/net/dev 2>/dev/null || true

sleep 1

emit boot_id_2
cat /proc/sys/kernel/random/boot_id 2>/dev/null || true
emit uptime_2
cat /proc/uptime 2>/dev/null || true
emit stat_2
cat /proc/stat 2>/dev/null || true
emit net_2
cat /proc/net/dev 2>/dev/null || true

emit meminfo
cat /proc/meminfo 2>/dev/null || true
emit df
LC_ALL=C df -l -P -k -T 2>/dev/null || true

exit 0
