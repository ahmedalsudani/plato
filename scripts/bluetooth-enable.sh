#!/bin/sh
# bluetooth-enable.sh -- bring up the MediaTek bluetooth stack on MT8113T Kobos
# (Clara BW / Clara Colour / Libra Colour) outside of Nickel, e.g. under Plato.
#
# Replicates what libnickel does at BT power-on, as recovered from the
# stock rootfs:
#   1. insmod /drivers/mt8113t-ntx/mt66xx/wmt_drv.ko        (WMT core)
#   2. /usr/bin/wmt_loader                                   (chip detect, exits)
#   3. /usr/bin/wmt_launcher -p /lib/firmware                (patch daemon, stays)
#   4. insmod /drivers/mt8113t-ntx/mt66xx/wmt_cdev_bt.ko     (creates /dev/stpbt)
#   5. start mtkbtd (D-Bus activated, owns com.kobo.mtk.bluedroid,
#      supervises /usr/bin/btservice, exposes org.bluez.* interfaces
#      on /org/bluez/hci0 and delivers HID keys via /dev/uinput)
#   6. BluedroidManager1.On() + Adapter1.Powered=true
#
# Usage:
#   bluetooth-enable.sh          bring everything up (idempotent)
#   bluetooth-enable.sh status   report state of each layer, change nothing
#   bluetooth-enable.sh off      power the adapter off (stack stays loaded)
#   bluetooth-enable.sh reset    off, then on again (recovers a wedged adapter)

DRIVER_DIR=/drivers/mt8113t-ntx/mt66xx
BUS_DEST=com.kobo.mtk.bluedroid
ADAPTER_PATH=/org/bluez/hci0
MANAGER_IFACE=com.kobo.bluetooth.BluedroidManager1

SCRIPTS_DIR=$(dirname "$0")

log() { echo "bt-enable: $*"; }

dbus_call() {
    # dbus_call <path> <interface.member> [args...]
    _path=$1; _member=$2; shift 2
    dbus-send --system --print-reply --dest=$BUS_DEST "$_path" "$_member" "$@"
}

mod_loaded() { grep -q "^$1 " /proc/modules; }

wait_for() {
    # wait_for <path> <seconds>
    _i=0
    while [ ! -e "$1" ]; do
        _i=$((_i + 1))
        [ $_i -ge $(($2 * 10)) ] && return 1
        usleep 100000
    done
    return 0
}

status() {
    for m in wmt_drv wmt_cdev_bt; do
        if mod_loaded $m; then log "module $m: loaded"; else log "module $m: NOT loaded"; fi
    done
    if pgrep -f wmt_launcher >/dev/null 2>&1; then
        log "wmt_launcher: running"
    else
        log "wmt_launcher: NOT running"
    fi
    [ -e /dev/stpbt ] && log "/dev/stpbt: present" || log "/dev/stpbt: MISSING"
    pgrep -f dbus-daemon >/dev/null 2>&1 && log "system dbus: running" || log "system dbus: NOT running"
    pgrep -f mtkbtd >/dev/null 2>&1 && log "mtkbtd: running" || log "mtkbtd: not running (starts on first D-Bus call)"
    pgrep -f btservice >/dev/null 2>&1 && log "btservice: running" || log "btservice: not running"
    if pgrep -f mtkbtd >/dev/null 2>&1; then
        log "adapter introspection ($ADAPTER_PATH):"
        dbus_call $ADAPTER_PATH org.freedesktop.DBus.Introspectable.Introspect 2>&1 | head -40
        log "Powered:"
        dbus_call $ADAPTER_PATH org.freedesktop.DBus.Properties.Get \
            string:org.bluez.Adapter1 string:Powered 2>&1 | tail -2
    fi
}

up_transport() {
    if ! mod_loaded wmt_drv; then
        log "loading wmt_drv.ko"
        insmod $DRIVER_DIR/wmt_drv.ko || { log "insmod wmt_drv.ko failed"; exit 1; }
        # chip detect; exits when done
        log "running wmt_loader"
        /usr/bin/wmt_loader >/dev/null 2>&1
    fi

    if ! pgrep -f wmt_launcher >/dev/null 2>&1; then
        log "starting wmt_launcher (firmware patch daemon)"
        /usr/bin/wmt_launcher -p /lib/firmware >/dev/null 2>&1 &
        sleep 1
    fi

    if ! mod_loaded wmt_cdev_bt; then
        log "loading wmt_cdev_bt.ko"
        insmod $DRIVER_DIR/wmt_cdev_bt.ko || { log "insmod wmt_cdev_bt.ko failed"; exit 1; }
    fi

    if ! wait_for /dev/stpbt 5; then
        log "ERROR: /dev/stpbt did not appear; check dmesg"
        exit 1
    fi
    log "/dev/stpbt is present"
}

up_daemon() {
    if ! pgrep -f dbus-daemon >/dev/null 2>&1; then
        log "system dbus not running; starting it"
        [ -s /var/lib/dbus/machine-id ] || dbus-uuidgen > /var/lib/dbus/machine-id
        dbus-daemon --system &
        sleep 1
    fi
    # Any call to the bus name auto-starts mtkbtd via
    # /usr/share/dbus-1/system-services/com.kobo.mtk.bluedroid.service
    log "pinging mtkbtd (D-Bus auto-start)"
    if ! dbus_call / org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then
        log "D-Bus activation failed; launching mtkbtd directly"
        /usr/local/Kobo/mtkbtd-launcher.sh >/dev/null 2>&1 &
        sleep 2
    fi
}

power_on() {
    # Nickel drives this via BluedroidManager1Proxy; the manager object's
    # path is not recoverable from strings, so try the likely paths.
    for p in / /org/bluez $ADAPTER_PATH; do
        if dbus_call $p $MANAGER_IFACE.On >/dev/null 2>&1; then
            log "BluedroidManager1.On() ok at $p"
            break
        fi
    done
    log "setting Adapter1.Powered = true"
    dbus_call $ADAPTER_PATH org.freedesktop.DBus.Properties.Set \
        string:org.bluez.Adapter1 string:Powered variant:boolean:true \
        || log "WARNING: could not set Powered (introspect with: $0 status)"
}

power_off() {
    dbus_call $ADAPTER_PATH org.freedesktop.DBus.Properties.Set \
        string:org.bluez.Adapter1 string:Powered variant:boolean:false
    for p in / /org/bluez $ADAPTER_PATH; do
        dbus_call $p $MANAGER_IFACE.Off >/dev/null 2>&1 && break
    done
}

bring_up() {
    PRE_UP_SCRIPT=$SCRIPTS_DIR/bluetooth-pre-up.sh
    [ -e "$PRE_UP_SCRIPT" ] && $PRE_UP_SCRIPT

    up_transport; up_daemon; power_on

    POST_UP_SCRIPT=$SCRIPTS_DIR/bluetooth-post-up.sh
    [ -e "$POST_UP_SCRIPT" ] && $POST_UP_SCRIPT
}

case "${1:-up}" in
    status) status ;;
    off)    power_off ;;
    reset)  power_off; sleep 2; bring_up ;;
    up)     bring_up ;;
    *)      echo "usage: $0 [up|status|off|reset]"; exit 2 ;;
esac
