#!/usr/bin/env bash
# Regenerates the README media from the real app: a throwaway HOME with demo
# hosts, an Alpine container as the SSH server, and vhs driving vssh.
# Needs: docker, vhs (with ttyd and ffmpeg), python3. Run from the repo root:
#   cargo build --release && docs/img/demo.sh
set -euo pipefail

D=/tmp/vssh-demo
docker rm -f vssh-demo-sshd >/dev/null 2>&1 || true
rm -rf "$D"
mkdir -p "$D/home/.ssh" "$D/work/site"
chmod 700 "$D/home/.ssh"
cp target/release/vssh "$D/vssh"
ssh-keygen -q -t ed25519 -f "$D/home/.ssh/id_ed25519" -N "" -C demo

# One container plays every reachable host; *.localhost resolves to loopback.
docker run -d --rm --name vssh-demo-sshd --hostname web-01 -p 127.0.0.1:2222:22 \
    -e PUB="$(cat "$D/home/.ssh/id_ed25519.pub")" alpine:latest sh -c '
    apk add --no-cache openssh >/dev/null 2>&1 && ssh-keygen -A >/dev/null &&
    mkdir -p /root/.ssh /root/app/releases /root/logs &&
    echo "$PUB" > /root/.ssh/authorized_keys && chmod 700 /root/.ssh &&
    echo "root:demo-password" | chpasswd 2>/dev/null && : > /etc/motd &&
    printf "#!/bin/sh\necho deploying\n" > /root/deploy.sh &&
    echo "release notes" > /root/NOTES.md &&
    head -c 48211 /dev/urandom > /root/backup.tar.gz &&
    exec /usr/sbin/sshd -D -e -o PermitRootLogin=yes' >/dev/null
until docker logs vssh-demo-sshd 2>&1 | grep -q "Server listening"; do sleep 0.5; done

host() { printf 'Host %s\n    HostName %s.localhost\n    Port %s\n    User %s\n%s\n' "$@"; }
{
    host web-01 web-01 2222 root "    LocalForward 8080 localhost:80"
    host web-02 web-02 2222 root ""
    host db-primary db 2222 root "    IdentityFile ~/.ssh/id_ed25519"
    host bastion bastion 2222 root ""
    host staging-api staging 2299 deploy ""   # nothing listens here: shown as down
    host backup-nas nas 2298 admin ""
    printf 'Host *\n    ServerAliveInterval 30\n'
} > "$D/home/.ssh/config"

echo "<h1>site</h1>" > "$D/work/index.html"
echo "todo" > "$D/work/README.md"
head -c 20480 /dev/urandom > "$D/work/logo.png"

# Groups, tags, notes, pins and snippets. Run by the tape after vssh has
# created its database, so the schema is always the app's own.
cat > "$D/meta.py" <<'PY'
import sqlite3
db = sqlite3.connect("/tmp/vssh-demo/home/.local/share/vssh/vssh.db")
hosts = {
    "web-01": ("prod", "web eu", 1, "nginx + app, behind the EU load balancer"),
    "web-02": ("prod", "web us", 0, ""),
    "db-primary": ("prod", "db postgres", 1, "primary, do not restart during business hours"),
    "bastion": ("infra", "jump", 0, ""),
    "staging-api": ("staging", "api", 0, ""),
    "backup-nas": ("infra", "storage", 0, ""),
}
for alias, (grp, tags, pinned, notes) in hosts.items():
    db.execute("UPDATE hosts SET grp=?, tags=?, pinned=?, notes=? WHERE alias=?",
               (grp, tags, pinned, notes, alias))
db.executemany("INSERT INTO snippets(host, name, command) VALUES (?, ?, ?)", [
    ("", "disk usage", "df -h /"),
    ("", "uptime", "uptime"),
    ("web-01", "reload nginx", "nginx -s reload"),
])
db.commit()
PY

vhs docs/img/demo.tape
docker rm -f vssh-demo-sshd >/dev/null
rm -rf "$D"
