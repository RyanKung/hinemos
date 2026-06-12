#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'USAGE'
Usage: install-host-database.sh [env-file]

Create the Postgres role and database used by the Hinemos host service.

Environment:
  HINEMOS_DB_NAME       Database name. Default: hinemos
  HINEMOS_DB_USER       Database role. Default: hinemos
  HINEMOS_DB_PASSWORD   Database password. Generated when omitted.

When [env-file] is provided, DATABASE_URL is written there with mode 0600.
USAGE
  exit 0
fi

env_file="${1:-}"
db_name="${HINEMOS_DB_NAME:-hinemos}"
db_user="${HINEMOS_DB_USER:-hinemos}"
db_password="${HINEMOS_DB_PASSWORD:-}"

if [[ ! "$db_name" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "invalid HINEMOS_DB_NAME: $db_name" >&2
  exit 1
fi

if [[ ! "$db_user" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "invalid HINEMOS_DB_USER: $db_user" >&2
  exit 1
fi

if [[ -z "$db_password" ]]; then
  if command -v openssl >/dev/null 2>&1; then
    db_password="$(openssl rand -hex 24)"
  else
    db_password="$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 48)"
  fi
fi

sql_literal() {
  printf "'%s'" "${1//\'/\'\'}"
}

postgres_psql() {
  if psql postgres --set=ON_ERROR_STOP=1 --command 'select 1' >/dev/null 2>&1; then
    psql "$@"
  elif [[ $EUID -eq 0 ]]; then
    runuser -u postgres -- psql "$@"
  elif [[ "$(id -un)" == "postgres" ]]; then
    psql "$@"
  else
    sudo -u postgres psql "$@"
  fi
}

role_literal="$(sql_literal "$db_user")"
db_literal="$(sql_literal "$db_name")"

postgres_psql postgres --set=ON_ERROR_STOP=1 <<SQL
do \$\$
begin
  if not exists (select 1 from pg_roles where rolname = ${role_literal}) then
    execute format('create role %I login password %L', ${db_user@Q}, ${db_password@Q});
  else
    execute format('alter role %I login password %L', ${db_user@Q}, ${db_password@Q});
  end if;
end
\$\$;

select format('create database %I owner %I', ${db_name@Q}, ${db_user@Q})
where not exists (select 1 from pg_database where datname = ${db_literal})\\gexec

select format('alter database %I owner to %I', ${db_name@Q}, ${db_user@Q})\\gexec
SQL

database_url="postgres://${db_user}:${db_password}@127.0.0.1:5432/${db_name}"

if [[ -n "$env_file" ]]; then
  env_dir="$(dirname "$env_file")"
  if [[ ! -d "$env_dir" ]]; then
    install -d -m 0755 "$env_dir"
  fi
  if [[ -f "$env_file" ]] && grep -q '^DATABASE_URL=' "$env_file"; then
    tmp_file="$(mktemp)"
    while IFS= read -r line || [[ -n "$line" ]]; do
      if [[ "$line" == DATABASE_URL=* ]]; then
        printf 'DATABASE_URL=%s\n' "$database_url"
      else
        printf '%s\n' "$line"
      fi
    done <"$env_file" >"$tmp_file"
    cat "$tmp_file" >"$env_file"
    rm -f "$tmp_file"
  else
    printf 'DATABASE_URL=%s\n' "$database_url" >>"$env_file"
  fi
  chmod 0600 "$env_file"
  chown root:root "$env_file" 2>/dev/null || true
  printf 'Wrote DATABASE_URL to %s\n' "$env_file"
else
  printf 'DATABASE_URL=%s\n' "$database_url"
fi
