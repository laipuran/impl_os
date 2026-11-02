#!/bin/sh
set -eu

# VS Code starts as dev and handles UID/GID mapping itself. The CLI starts
# briefly as root with explicit host IDs, then drops privileges before work.
if [ "$(id -u)" -eq 0 ]; then
    : "${LOCAL_UID:?start with LOCAL_UID and LOCAL_GID, or use just container}"
    : "${LOCAL_GID:?start with LOCAL_UID and LOCAL_GID, or use just container}"
    case "$LOCAL_UID:$LOCAL_GID" in
        *[!0-9:]*|:*|*:) echo 'UID/GID must be decimal integers' >&2; exit 2 ;;
    esac
    if [ "$LOCAL_UID" -eq 0 ] || [ "$LOCAL_GID" -eq 0 ]; then
        echo 'Run just container as an ordinary host user, not root' >&2
        exit 2
    fi
    old_uid=$(id -u dev)
    old_gid=$(id -g dev)
    if [ "$old_gid" -ne "$LOCAL_GID" ]; then
        if ! getent group "$LOCAL_GID" >/dev/null; then
            groupmod --gid "$LOCAL_GID" dev
        fi
        usermod --gid "$LOCAL_GID" dev
    fi
    if [ "$old_uid" -ne "$LOCAL_UID" ]; then
        # Fail on an existing UID rather than deleting or repurposing accounts.
        usermod --uid "$LOCAL_UID" dev
    fi
    if [ "$old_uid:$old_gid" != "$LOCAL_UID:$LOCAL_GID" ]; then
        # Never chown the bind-mounted workspace as a startup side effect.
        chown -R "$LOCAL_UID:$LOCAL_GID" /home/dev
    fi
    exec setpriv --reuid=dev --regid="$LOCAL_GID" --init-groups --no-new-privs "$@"
fi

exec "$@"
