# Acceptance reads working files. Refuse a different index before and after it.
IRONLINT_TREE="$(git write-tree)" || exit 1
if [ ! -f "$ROOT/.ironlint.yml" ] || ! git cat-file -e "$IRONLINT_TREE:.ironlint.yml"; then
  echo "ironlint: policy must be present in both index and working tree; blocking commit" >&2
  exit 1
fi
ironlint_index_matches() (
  cd "$ROOT" || exit 1
  [ "$(git write-tree)" = "$IRONLINT_TREE" ] || exit 1
  LIST="$(mktemp "${TMPDIR:-/tmp}/ironlint-index.XXXXXX")" || exit 1
  trap 'rm -f "$LIST"' EXIT
  trap 'exit 1' HUP INT TERM
  git ls-tree -r -z "$IRONLINT_TREE" > "$LIST" || exit 1
  # NUL-delimited records preserve spaces, tabs, and newlines in file names.
  # Hash raw bytes: diff's clean filters and index flags can hide mismatches.
  xargs -0 sh -c '
    for entry do
      tab="$(printf "\t")"
      meta=${entry%%"$tab"*}
      file=${entry#*"$tab"}
      mode=${meta%% *}
      oid=${meta##* }
      parent=$file
      while :; do
        case "$parent" in */*) parent=${parent%/*} ;; *) break ;; esac
        [ ! -L "$parent" ] || exit 1
      done
      if [ "$mode" = 120000 ]; then
        [ -L "$file" ] || exit 1
        # -n emits exact target bytes; the sentinel preserves trailing newlines
        # through command substitution. ./ protects leading-dash file names.
        target=$(readlink -n "./$file" && printf .) || exit 1
        actual=$(printf %s "${target%?}" | git hash-object --stdin) || exit 1
      else
        [ -f "$file" ] && [ ! -L "$file" ] || exit 1
        case "$mode" in
          100644) [ ! -x "$file" ] || exit 1 ;;
          100755) [ -x "$file" ] || exit 1 ;;
          *) exit 1 ;;
        esac
        actual=$(git hash-object --no-filters -- "$file") || exit 1
      fi
      [ "$actual" = "$oid" ] || exit 1
    done
  ' sh < "$LIST" || exit 1
  # A staged deletion must not remain available to acceptance on disk.
  git diff --cached --name-only --diff-filter=D --no-renames -z > "$LIST" || exit 1
  xargs -0 sh -c '
    tree=$1
    shift
    for file do
      if [ -d "$file" ] && [ ! -L "$file" ] && [ "$(git cat-file -t "$tree:$file")" = tree ]; then
        continue
      fi
      [ ! -e "$file" ] && [ ! -L "$file" ] || exit 1
    done
  ' sh "$IRONLINT_TREE" < "$LIST"
)
ironlint_index_matches || {
  echo "ironlint: staged and working files differ or use unsupported entries; stage or stash changes before committing" >&2
  exit 1
}
