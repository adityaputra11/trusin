#!/usr/bin/env bash
set -euo pipefail

release_id=${1:?release ID is required}
deploy_user=${2:?deploy user is required}
stage_dir=${3:?staged release directory is required}
release_dir="/opt/trusin/releases/${release_id}"
source_dir="$(getent passwd "$deploy_user" | cut -d: -f6)/terusin"

if ! id -u terusin >/dev/null 2>&1; then
  useradd --system --no-create-home --shell /usr/sbin/nologin terusin
fi

if [ -d "$source_dir/.git" ]; then
  runuser -u "$deploy_user" -- git -C "$source_dir" remote set-url origin https://github.com/adityaputra11/trusin.git
  runuser -u "$deploy_user" -- git -C "$source_dir" checkout master
  runuser -u "$deploy_user" -- git -C "$source_dir" pull --ff-only origin master
fi

install -d -o terusin -g terusin -m 0755 /opt/trusin/releases /etc/trusin
install -d -o root -g root -m 0755 /var/www/trusin
rm -rf "$release_dir"
install -d -o terusin -g terusin -m 0755 "$release_dir"
cp -a "$stage_dir/bin" "$release_dir/"
chown -R terusin:terusin "$release_dir"
chmod +x "$release_dir/bin/backend" "$release_dir/bin/web"

install -m 0600 -o terusin -g terusin "$stage_dir/runtime.env" /etc/trusin/trusin.env

install -d -o root -g root -m 0755 /var/www/trusin/landing /var/www/trusin/docs
rm -rf /var/www/trusin/landing/* /var/www/trusin/docs/*
cp -a "$stage_dir/landing/." /var/www/trusin/landing/
cp -a "$stage_dir/docs/." /var/www/trusin/docs/
chown -R root:root /var/www/trusin
chmod -R a+rX /var/www/trusin

install -m 0644 "$stage_dir/deploy/terusin-backend.service" /etc/systemd/system/terusin-backend.service
install -m 0644 "$stage_dir/deploy/terusin-web.service" /etc/systemd/system/terusin-web.service
install -d -o root -g root -m 0755 /etc/caddy/sites-enabled
install -m 0644 "$stage_dir/deploy/trusin.caddy" /etc/caddy/sites-enabled/trusin.caddy
if ! grep -Fqx 'import /etc/caddy/sites-enabled/*' /etc/caddy/Caddyfile; then
  printf '\nimport /etc/caddy/sites-enabled/*\n' >> /etc/caddy/Caddyfile
fi

ln -sfn "$release_dir" /opt/trusin/current
caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
systemctl daemon-reload
systemctl enable terusin-backend terusin-web
systemctl restart terusin-backend terusin-web
systemctl reload caddy
