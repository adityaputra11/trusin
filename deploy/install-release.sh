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

if [ -d "$stage_dir/bin" ]; then
  previous_release="$(readlink -f /opt/trusin/current 2>/dev/null || true)"
  rm -rf "$release_dir"
  install -d -o terusin -g terusin -m 0755 "$release_dir/bin"
  if [ -n "$previous_release" ] && [ "$previous_release" != "$release_dir" ] && [ -d "$previous_release/bin" ]; then
    cp -a "$previous_release/bin/." "$release_dir/bin/"
  fi
  cp -a "$stage_dir/bin/." "$release_dir/bin/"
  chown -R terusin:terusin "$release_dir"
  chmod +x "$release_dir/bin"/*
  ln -sfn "$release_dir" /opt/trusin/current
fi

if [ -f "$stage_dir/runtime.env" ]; then
  install -m 0600 -o terusin -g terusin "$stage_dir/runtime.env" /etc/trusin/trusin.env
fi

for site in landing docs download; do
  if [ -d "$stage_dir/$site" ]; then
    install -d -o root -g root -m 0755 "/var/www/trusin/$site"
    rm -rf "/var/www/trusin/$site"/*
    cp -a "$stage_dir/$site/." "/var/www/trusin/$site/"
  fi
done
chown -R root:root /var/www/trusin
chmod -R a+rX /var/www/trusin

if [ -f "$stage_dir/deploy/terusin-backend.service" ]; then
  install -m 0644 "$stage_dir/deploy/terusin-backend.service" /etc/systemd/system/terusin-backend.service
  install -m 0644 "$stage_dir/deploy/terusin-web.service" /etc/systemd/system/terusin-web.service
  install -d -o root -g root -m 0755 /etc/caddy/sites-enabled
  install -m 0644 "$stage_dir/deploy/trusin.caddy" /etc/caddy/sites-enabled/trusin.caddy
  if ! grep -Fqx 'import /etc/caddy/sites-enabled/*' /etc/caddy/Caddyfile; then
    printf '\nimport /etc/caddy/sites-enabled/*\n' >> /etc/caddy/Caddyfile
  fi
  caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
  systemctl daemon-reload
  systemctl enable terusin-backend terusin-web
  systemctl reload caddy
fi

if [ -f "$stage_dir/bin/backend" ]; then
  systemctl restart terusin-backend
fi
if [ -f "$stage_dir/bin/web" ]; then
  systemctl restart terusin-web
fi
