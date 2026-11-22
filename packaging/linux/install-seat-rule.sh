# Invoked by the desktop with the embedded rule as its sole argument.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
umask 022
cd /
command -v udevadm >/dev/null
test -x /usr/bin/sha256sum
rules=/etc/udev/rules.d
mkdir -p "$rules"
stage=$(mktemp "$rules/.byakko-seat.XXXXXXXX")
trap 'rm -f "$stage"' EXIT
printf '%s\n' "$1" > "$stage"
chmod 644 "$stage"
mv -fT "$stage" "$rules/71-byakko-seat.rules"
if test -n "${2:-}"; then
    stage=$(mktemp "$rules/.byakko-events.XXXXXXXX")
    printf '%s\n' "$2" > "$stage"
    chmod 644 "$stage"
    mv -fT "$stage" "$rules/71-byakko-receiver-events.rules"
fi
udevadm control --reload-rules
# The reloaded rule checks each candidate's descriptor before granting access.
for node in /sys/class/hidraw/hidraw*; do
    test -f "$node/device/uevent" || continue
    grep -Eiq '^HID_ID=0003:00003151:000040(11|15)$' "$node/device/uevent" || continue
    udevadm trigger --settle --action=change "$node"
done
