# Invoked by the desktop with the embedded rule as its sole argument.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
umask 022
cd /
command -v udevadm >/dev/null
test -x /usr/bin/cmp
dir=/etc/udev/byakko
rules=/etc/udev/rules.d
mkdir -p "$dir" "$rules"
stage=$(mktemp -d "$dir/setup.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT
# Exactly one vendor feature report. The two observed descriptors differ
# only in the ordering of Report Size and Report Count.
for form in a b; do
    case "$form" in
        a) shape='\165\010\225\100' ;;
        b) shape='\225\100\165\010' ;;
    esac
    printf '%b' "\006\377\377\011\002\241\001\011\002\025\200\045\177${shape}\261\002\300" > "$stage/config-$form"
    chmod 644 "$stage/config-$form"
    mv -fT "$stage/config-$form" "$dir/config-$form"
done
printf '%s\n' "$1" > "$stage/71-byakko-seat.rules"
chmod 644 "$stage/71-byakko-seat.rules"
mv -fT "$stage/71-byakko-seat.rules" "$rules/71-byakko-seat.rules"
udevadm control --reload-rules
# Apply only to supported USB HID devices with a matching descriptor.
for node in /sys/class/hidraw/hidraw*; do
    test -f "$node/device/uevent" || continue
    grep -Eiq '^HID_ID=0003:00003151:000040(11|15)$' "$node/device/uevent" || continue
    if cmp "$node/device/report_descriptor" "$dir/config-a" >/dev/null 2>&1 || cmp "$node/device/report_descriptor" "$dir/config-b" >/dev/null 2>&1; then
        udevadm trigger --action=change "$node"
    fi
done
