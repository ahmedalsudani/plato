#! /bin/sh

SCRIPTS_DIR=$(dirname "$0")
PRE_DOWN_SCRIPT=$SCRIPTS_DIR/bluetooth-pre-down.sh
[ -e "$PRE_DOWN_SCRIPT" ] && $PRE_DOWN_SCRIPT
POST_DOWN_SCRIPT=$SCRIPTS_DIR/bluetooth-post-down.sh
[ -e "$POST_DOWN_SCRIPT" ] && $POST_DOWN_SCRIPT
