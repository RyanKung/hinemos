# HAProxy Edge Deployment for Hinemos

This document describes the recommended sanitized host deployment for Hinemos when HAProxy owns the public TCP/TLS entrypoints.

## Goals

- Publish Hinemos over SSH on TCP `22`.
- Keep system administration SSH on TCP `2222`.
- Publish Hinemos IMAP/SMTP through TLS on TCP `993` and `465`.
- Keep Hinemos backend services bound to loopback addresses.
- Use Let's Encrypt certificates and automatic renewal.
- Avoid exposing secrets, database credentials, private keys, or mail tokens.

## Public Ports

| Port | Public Service | Frontend Owner | Backend |
| --- | --- | --- | --- |
| `22/tcp` | Hinemos SSH world | HAProxy | `127.0.0.1:2022` |
| `465/tcp` | Hinemos SMTPS | HAProxy TLS | `127.0.0.1:2525` |
| `993/tcp` | Hinemos IMAPS | HAProxy TLS | `127.0.0.1:2143` |
| `2222/tcp` | Host administration SSH | OpenSSH sshd | direct sshd listener |

Recommended security group policy:

- `22/tcp`: public, because this is the Hinemos product entrypoint.
- `465/tcp` and `993/tcp`: public if remote mail clients/agents need access.
- `2222/tcp`: restrict to administrator IPs whenever possible.

## DNS

Use DNS-only records for TCP services. Standard Cloudflare orange-cloud proxy does not proxy SSH, SMTP, or IMAP.

Minimum:

```text
hinemos.ai A <server-public-ip> DNS only
```

Optional service names once certificates are expanded:

```text
imap.hinemos.ai A <server-public-ip> DNS only
smtp.hinemos.ai A <server-public-ip> DNS only
```

## Hinemos Environment

`/etc/hinemos/hinemos.env` should contain sanitized values shaped like:

```env
DATABASE_URL=postgres://<user>:<password>@127.0.0.1:5432/<database>
HINEMOS_BIND=127.0.0.1:2022
HINEMOS_WORLD=/opt/hinemos/worlds/sample
HINEMOS_HOST_KEY=/var/lib/hinemos/ssh_host_ed25519_key
HINEMOS_ADMIN_SOCKET=/run/hinemos/admin.sock
HINEMOS_MAIL_DOMAIN=hinemos.local
HINEMOS_SMTP_BIND=127.0.0.1:2525
HINEMOS_IMAP_BIND=127.0.0.1:2143
BLACKSTONE_AGENT_ONLINE=1
BLACKSTONE_LLM_ENABLED=1
BLACKSTONE_LLM_BASE_URL=http://127.0.0.1:14550
BLACKSTONE_LLM_AUTH_TOKEN=<secret>
BLACKSTONE_LLM_MODEL=gpt-5.3-codex-spark
```

Permissions:

```bash
sudo chown root:root /etc/hinemos/hinemos.env
sudo chmod 600 /etc/hinemos/hinemos.env
```

## System Services

Hinemos SSH daemon:

```text
/etc/systemd/system/hinemos.service
```

Hinemos mail sidecar:

```text
/etc/systemd/system/hinemos-mail.service
```

HAProxy:

```text
/etc/haproxy/haproxy.cfg
```

Enable and start:

```bash
sudo systemctl enable hinemos.service hinemos-mail.service haproxy.service
sudo systemctl restart hinemos.service hinemos-mail.service haproxy.service
```

## Certificate Issuance

Use Let's Encrypt with Cloudflare DNS-01 validation. Store the Cloudflare API token in:

```text
/etc/letsencrypt/cloudflare.ini
```

Expected file shape:

```ini
dns_cloudflare_api_token = <cloudflare-api-token>
```

Permissions:

```bash
sudo chown root:root /etc/letsencrypt/cloudflare.ini
sudo chmod 600 /etc/letsencrypt/cloudflare.ini
```

Issue a certificate:

```bash
sudo certbot certonly \
  --dns-cloudflare \
  --dns-cloudflare-credentials /etc/letsencrypt/cloudflare.ini \
  --non-interactive \
  --agree-tos \
  --register-unsafely-without-email \
  --cert-name hinemos.ai \
  -d hinemos.ai
```

If `imap.hinemos.ai` and `smtp.hinemos.ai` are delegated in Cloudflare DNS, expand the certificate:

```bash
sudo certbot certonly \
  --dns-cloudflare \
  --dns-cloudflare-credentials /etc/letsencrypt/cloudflare.ini \
  --cert-name hinemos.ai \
  --expand \
  -d hinemos.ai \
  -d imap.hinemos.ai \
  -d smtp.hinemos.ai
```

## HAProxy Certificate Bundle

HAProxy uses a combined certificate and key file:

```bash
sudo mkdir -p /etc/haproxy/certs
sudo sh -c 'cat \
  /etc/letsencrypt/live/hinemos.ai/fullchain.pem \
  /etc/letsencrypt/live/hinemos.ai/privkey.pem \
  > /etc/haproxy/certs/hinemos-mail.pem'
sudo chmod 600 /etc/haproxy/certs/hinemos-mail.pem
sudo chown root:root /etc/haproxy/certs/hinemos-mail.pem
```

## HAProxy Configuration

Recommended `/etc/haproxy/haproxy.cfg`:

```haproxy
global
    log /dev/log local0
    log /dev/log local1 notice
    chroot /var/lib/haproxy
    stats socket /run/haproxy/admin.sock mode 660 level admin
    stats timeout 30s
    user haproxy
    group haproxy
    daemon
    maxconn 4000

defaults
    log global
    mode tcp
    option tcplog
    timeout connect 5s
    timeout client 1h
    timeout server 1h
    timeout tunnel 24h

frontend hinemos_ssh
    bind *:22
    mode tcp
    stick-table type ip size 100k expire 10m store conn_rate(60s),conn_cur
    tcp-request connection track-sc0 src
    tcp-request connection reject if { sc_conn_rate(0) gt 20 }
    tcp-request connection reject if { sc_conn_cur(0) gt 8 }
    default_backend hinemos_ssh_backend

backend hinemos_ssh_backend
    mode tcp
    server hinemos_ssh 127.0.0.1:2022 check

frontend hinemos_imaps
    bind *:993 ssl crt /etc/haproxy/certs/hinemos-mail.pem
    mode tcp
    stick-table type ip size 100k expire 10m store conn_rate(60s),conn_cur
    tcp-request connection track-sc0 src
    tcp-request connection reject if { sc_conn_rate(0) gt 30 }
    tcp-request connection reject if { sc_conn_cur(0) gt 20 }
    default_backend hinemos_imap_backend

backend hinemos_imap_backend
    mode tcp
    server hinemos_imap 127.0.0.1:2143 check

frontend hinemos_smtps
    bind *:465 ssl crt /etc/haproxy/certs/hinemos-mail.pem
    mode tcp
    stick-table type ip size 100k expire 10m store conn_rate(60s),conn_cur
    tcp-request connection track-sc0 src
    tcp-request connection reject if { sc_conn_rate(0) gt 30 }
    tcp-request connection reject if { sc_conn_cur(0) gt 10 }
    default_backend hinemos_smtp_backend

backend hinemos_smtp_backend
    mode tcp
    server hinemos_smtp 127.0.0.1:2525 check
```

Validate and restart:

```bash
sudo haproxy -c -f /etc/haproxy/haproxy.cfg
sudo systemctl restart haproxy.service
```

## Certificate Renewal Hook

Install a Certbot deploy hook so renewed certificates are copied into HAProxy format:

```text
/etc/letsencrypt/renewal-hooks/deploy/reload-haproxy-hinemos.sh
```

Hook content:

```bash
#!/usr/bin/env bash
set -euo pipefail
cat /etc/letsencrypt/live/hinemos.ai/fullchain.pem /etc/letsencrypt/live/hinemos.ai/privkey.pem > /etc/haproxy/certs/hinemos-mail.pem
chmod 600 /etc/haproxy/certs/hinemos-mail.pem
chown root:root /etc/haproxy/certs/hinemos-mail.pem
systemctl reload haproxy.service
```

Permissions:

```bash
sudo chmod +x /etc/letsencrypt/renewal-hooks/deploy/reload-haproxy-hinemos.sh
```

## Validation

Check listening sockets:

```bash
sudo ss -ltnp '( sport = :22 or sport = :2022 or sport = :465 or sport = :993 or sport = :2143 or sport = :2525 )'
```

Expected shape:

```text
0.0.0.0:22       haproxy
0.0.0.0:465      haproxy
0.0.0.0:993      haproxy
127.0.0.1:2022   hinemos
127.0.0.1:2143   hinemos
127.0.0.1:2525   hinemos
```

Test Hinemos SSH banner:

```bash
timeout 8 bash -lc 'exec 3<>/dev/tcp/127.0.0.1/22; IFS= read -r line <&3; printf "%s\n" "$line"'
```

Expected:

```text
SSH-2.0-russh_...
```

Test IMAPS:

```bash
openssl s_client -connect hinemos.ai:993 -servername hinemos.ai -brief
```

Test SMTPS:

```bash
openssl s_client -connect hinemos.ai:465 -servername hinemos.ai -brief
```

## Client Configuration

Hinemos SSH:

```bash
ssh <user>@hinemos.ai
```

System administration SSH:

```bash
ssh -p 2222 admin@hinemos.ai
```

IMAP:

```text
Host: hinemos.ai
Port: 993
Security: SSL/TLS
Username: <Hinemos username>
Password: <mail-token from /settings mail-token>
```

SMTP:

```text
Host: hinemos.ai
Port: 465
Security: SSL/TLS
Username: <Hinemos username>
Password: <mail-token from /settings mail-token>
```

## Notes

- Do not expose Hinemos internal mail ports `2143` or `2525` publicly.
- Do not commit `/etc/hinemos/hinemos.env`, Cloudflare credentials, private keys, or mail tokens.
- Cloudflare standard orange-cloud proxy does not proxy SSH, SMTP, or IMAP. Use DNS-only records unless using Cloudflare Tunnel or Spectrum.
- If admin SSH on `2222` is exposed, restrict it to trusted administrator IPs whenever possible.
