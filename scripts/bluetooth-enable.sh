#! /bin/sh

SCRIPTS_DIR=$(dirname "$0")
PRE_UP_SCRIPT=$SCRIPTS_DIR/bluetooth-pre-up.sh
[ -e "$PRE_UP_SCRIPT" ] && $PRE_UP_SCRIPT
POST_UP_SCRIPT=$SCRIPTS_DIR/bluetooth-post-up.sh
[ -e "$POST_UP_SCRIPT" ] && $POST_UP_SCRIPT
