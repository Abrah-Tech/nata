#!/usr/bin/env bash
#
# Backup drill for Nata's Postgres.
#
# A dump nobody has restored is a guess, not a backup. This proves one: it dumps
# the database, restores it into a throwaway copy beside it, compares the schema
# and the data against the source, then drops the copy. The source is only ever
# read, and a failure leaves nothing behind.
#
# What is compared:
#   - the schema: every column's name, type, and nullability
#   - RLS: which tables force it, and every policy with its command and
#     expressions — the tenancy boundary has to survive the restore
#   - an exact row count per table
#   - every column's value: numeric columns are summed exactly, anything else is
#     hashed row by row and the hashes summed, so a changed name or date is
#     caught just like a changed amount
#
# `_sqlx_migrations` is part of that fingerprint, so a restored copy reporting a
# different migration level fails the drill. Run with `--self-test` to prove the
# comparison actually notices a change.
#
# The pg_dump major version has to equal the server's. An older client can miss
# objects; a newer one writes settings the server rejects on restore (pg_dump 17
# and later emit `SET transaction_timeout = 0`, which Postgres 16 fails on).
#
# Usage:
#   DATABASE_URL=postgres://user:pass@host:5432/nata deploy/vps/backup.sh
#
# Options:
#   --url URL      source connection string (default: DATABASE_URL)
#   --output DIR   where to write the dump (default: ./tmp)
#   --keep         keep the dump after a successful run
#   --self-test    change a row in the restored copy and require the comparison
#                  to fail; use it to check the drill itself
#
# Needs pg_dump, pg_restore, and psql, plus a role allowed to create databases.

set -euo pipefail

export PGCONNECT_TIMEOUT="${PGCONNECT_TIMEOUT:-10}"

url="${DATABASE_URL:-}"
output_dir="./tmp"
keep=false
self_test=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --url)
      url="${2:?--url needs a connection string}"
      shift 2
      ;;
    --output)
      output_dir="${2:?--output needs a directory}"
      shift 2
      ;;
    --keep)
      keep=true
      shift
      ;;
    --self-test)
      self_test=true
      shift
      ;;
    -h | --help)
      sed -n '2,32p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [ -z "$url" ]; then
  echo "DATABASE_URL is not set; pass --url or export it" >&2
  exit 2
fi

for tool in pg_dump pg_restore psql; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required" >&2
    exit 2
  fi
done

# A pg_dump older than the server refuses, and one that is merely close enough
# to run can still miss things. Refuse rather than produce a dump nobody will
# discover is short until it is too late. The newer client is refused for the
# opposite reason: pg_dump 17+ writes `SET transaction_timeout = 0` whatever the
# server version, and a server older than 17 rejects it halfway through the
# restore — so a client that is too new fails louder than a too-old one.
server_major="$(( "$(psql "$url" -At -c "select current_setting('server_version_num')")" / 10000 ))"
client_major="$(pg_dump --version | grep -oE '[0-9]+' | head -1)"

if [ "$client_major" -lt "$server_major" ]; then
  echo "FAIL: pg_dump $client_major is older than the server (Postgres $server_major)" >&2
  echo "install a PostgreSQL $server_major client, or point --url at a server matching pg_dump" >&2
  exit 1
fi

if [ "$client_major" -gt "$server_major" ]; then
  echo "FAIL: pg_dump $client_major is newer than the server (Postgres $server_major)" >&2
  echo "its dump can carry settings this server rejects on restore (pg_dump 17+ writes SET transaction_timeout)" >&2
  echo "install a PostgreSQL $server_major client, or point --url at a server matching pg_dump" >&2
  exit 1
fi

# The scratch database sits beside the source so the restore reuses the same
# host, role, and options. Splitting off a query string keeps `sslmode=require`
# and friends intact.
if [[ "$url" == *"?"* ]]; then
  base="${url%%\?*}"
  query="?${url#*\?}"
else
  base="$url"
  query=""
fi

source_db="${base##*/}"
scratch_db="${source_db}_restore_check"
scratch_url="${base%/*}/${scratch_db}${query}"

if [ "$source_db" = "$scratch_db" ]; then
  echo "refusing to use $scratch_db as the source" >&2
  exit 2
fi

mkdir -p "$output_dir"
dump_file="${output_dir}/nata-$(date +%Y%m%d-%H%M%S).dump"
work="$(mktemp -d)"
scratch_created=false

cleanup() {
  if [ "$scratch_created" = true ]; then
    psql "$url" -v ON_ERROR_STOP=1 \
      -c "drop database if exists \"$scratch_db\"" >/dev/null 2>&1 || true
  fi
  rm -rf "$work"
  if [ "$keep" = false ]; then
    rm -f "$dump_file"
  fi
}
trap cleanup EXIT

echo "==> source:      $source_db"
echo "==> scratch:     $scratch_db"
echo "==> dump:        $dump_file"

echo
echo "==> dumping"
pg_dump "$url" --format=custom --file="$dump_file"

if [ ! -s "$dump_file" ]; then
  echo "FAIL: the dump is empty" >&2
  exit 1
fi

echo
echo "==> restoring into the scratch database"
psql "$url" -v ON_ERROR_STOP=1 -c "drop database if exists \"$scratch_db\"" >/dev/null
psql "$url" -v ON_ERROR_STOP=1 -c "create database \"$scratch_db\"" >/dev/null
scratch_created=true

# Owner and privilege grants are deliberately skipped: the scratch copy is read
# by the same role that created it, and carrying grants over says nothing about
# whether the data survived.
pg_restore --dbname="$scratch_url" --no-owner --no-privileges "$dump_file"

# The checks are generated from the source, so a table the restore lost makes
# the comparison fail instead of being silently skipped.
table_count=0
counts_sql=""
while IFS= read -r table; do
  [ -n "$table" ] || continue
  table_count=$((table_count + 1))
  if [ -n "$counts_sql" ]; then
    counts_sql="$counts_sql union all "
  fi
  counts_sql="${counts_sql}select '$table' as label, count(*) as value from public.\"$table\""
done < <(psql "$url" -At -c \
  "select tablename from pg_tables where schemaname = 'public' order by tablename")

# Every column gets a value check: numeric columns are summed exactly, anything
# else is hashed per row and the hashes summed, so a changed name or date is
# caught just like a changed amount.
values_sql=""
while IFS='|' read -r table column type; do
  [ -n "$table" ] || continue
  if [ -n "$values_sql" ]; then
    values_sql="$values_sql union all "
  fi

  if [ "$type" = "numeric" ]; then
    expression="sum(\"$column\")"
  else
    expression="sum(hashtextextended(\"$column\"::text, 0))"
  fi

  values_sql="${values_sql}select '$table.$column' as label, $expression as value from public.\"$table\""
done < <(psql "$url" -At -F'|' -c \
  "select table_name, column_name, data_type from information_schema.columns
   where table_schema = 'public'
   order by table_name, column_name")

cat >"$work/verify.sql" <<SQL
select 'schema:' || table_name || '.' || column_name || '|' ||
       data_type || ':' || is_nullable
  from information_schema.columns
 where table_schema = 'public'
 order by table_name, column_name;

-- RLS is the tenancy boundary, and a restore that drops a policy turns into a
-- cross-tenant leak rather than a data loss. So the policies are fingerprinted
-- too, not just the tables they hang off.
select 'rls:' || relname || '|' || relrowsecurity || ':' || relforcerowsecurity
  from pg_class c
  join pg_namespace n on n.oid = c.relnamespace
 where n.nspname = 'public' and c.relkind = 'r'
 order by relname;

select 'policy:' || schemaname || '.' || tablename || '.' || policyname || '|' ||
       cmd || ':' || roles::text || ':' || coalesce(qual, '') || ':' || coalesce(with_check, '')
  from pg_policies
 where schemaname = 'public'
 order by tablename, policyname;

select 'count:' || label || '|' || value from ($counts_sql) s order by label;

select 'value:' || label || '|' || value from ($values_sql) s order by label;
SQL

# A drill that cannot fail is worthless. `--self-test` restores, changes one row
# in a table with no RLS, and requires the comparison to notice.
if [ "$self_test" = true ]; then
  echo
  echo "==> self-test: changing a row in the restored copy"
  # `units` is the one public table with no RLS and no tenant, so the drill can
  # write to it without already knowing a tenant. If it ever gains RLS, this is
  # the line to revisit.
  psql "$scratch_url" -v ON_ERROR_STOP=1 \
    -c "insert into public.units (id, code, name) values (gen_random_uuid(), 'selftest', 'self-test')" >/dev/null
fi

echo
echo "==> comparing source and restored copy"
psql "$url" -At -v ON_ERROR_STOP=1 -f "$work/verify.sql" >"$work/source.txt"
psql "$scratch_url" -At -v ON_ERROR_STOP=1 -f "$work/verify.sql" >"$work/restored.txt"

checks="$(wc -l <"$work/source.txt")"
rows="$(awk -F'|' '$1 ~ /^count:/ { total += $2 } END { print total + 0 }' "$work/source.txt")"

if diff -u "$work/source.txt" "$work/restored.txt" >"$work/diff.txt"; then
  if [ "$self_test" = true ]; then
    echo "FAIL: the comparison did not notice a deliberate change" >&2
    exit 1
  fi

  echo
  echo "PASS: $checks checks over $table_count tables and $rows rows"
  if [ "$keep" = true ]; then
    echo "the scratch database has been dropped; the dump is kept at $dump_file"
  else
    echo "the scratch database has been dropped and the dump removed"
  fi
  exit 0
fi

if [ "$self_test" = true ]; then
  echo
  echo "PASS: the comparison caught the deliberate change"
  head -20 "$work/diff.txt"
  exit 0
fi

echo "FAIL: the restored copy differs from the source" >&2
cat "$work/diff.txt" >&2
exit 1
