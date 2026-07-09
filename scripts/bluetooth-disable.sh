#!/bin/sh
# bluetooth-disable.sh -- tear down the bluetooth stack brought up by
# bluetooth-enable.sh on MT8113T Kobos.
#
# Powers the adapter off, stops the watchdog and the bluedroid daemons,
# and unloads wmt_cdev_bt.ko (removes /dev/stpbt). Deliberately leaves
# wmt_drv.ko and wmt_launcher alone: the WMT core is shared with WiFi
# (wlan_drv_gen4m / wmt_chrdev_wifi), so removing it would kill WiFi too.
#
# Note: mtkbtd is D-Bus activated, so any later call to the
# com.kobo.mtk.bluedroid bus name (e.g. from the watchdog, if it is
# still running) will restart it. That is why the watchdog is killed
# first.

BUS_DEST=com.kobo.mtk.bluedroid
ADAPTER_PATH=/org/bluez/hci0
MANAGER_IFACE=com.kobo.bluetooth.BluedroidManager1

log() { echo "bt-disable: $*"; }

SCRIPTS_DIR=$(dirname "$0")
PRE_DOWN_SCRIPT=$SCRIPTS_DIR/bluetooth-pre-down.sh
[ -e "$PRE_DOWN_SCRIPT" ] && $PRE_DOWN_SCRIPT

pkill -f bt-watchdog.sh 2>/dev/null && log "stopped bt-watchdog"

if pgrep -f mtkbtd >/dev/null 2>&1; then
    log "powering adapter off"
    dbus-send --system --print-reply --dest=$BUS_DEST $ADAPTER_PATH \
        org.freedesktop.DBus.Properties.Set \
        string:org.bluez.Adapter1 string:Powered variant:boolean:false \
        >/dev/null 2>&1
    for p in / /org/bluez $ADAPTER_PATH; do
        dbus-send --system --print-reply --dest=$BUS_DEST $p \
            $MANAGER_IFACE.Off >/dev/null 2>&1 && break
    done
    sleep 1
    log "stopping mtkbtd/btservice"
    pkill -f mtkbtd 2>/dev/null
    pkill -f btservice 2>/dev/null
    sleep 1
fi

if grep -q '^wmt_cdev_bt ' /proc/modules; then
    if rmmod wmt_cdev_bt; then
        log "unloaded wmt_cdev_bt.ko"
    else
        log "WARNING: rmmod wmt_cdev_bt failed (still in use?)"
    fi
fi

POST_DOWN_SCRIPT=$SCRIPTS_DIR/bluetooth-post-down.sh
[ -e "$POST_DOWN_SCRIPT" ] && $POST_DOWN_SCRIPT

log "done"
