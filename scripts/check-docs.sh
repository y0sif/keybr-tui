#!/usr/bin/env bash
# Docs check (CI and `just check` run it): keeps ignored files out of the index, markdown to the
# allowed paths, CLAUDE.md files to one import, private data out of every text file, dashes out
# of the markdown, history and provenance out of the user docs and AGENTS.md files, and each doc
# under its line budget. Prints one `path:line: rule: excerpt` line per violation, then a count,
# and exits 1 when there is any. Untracked files that are not ignored are checked too, so an
# internal file fails here before it is committed. It runs on bash 3.2 (stock macOS), so it reads
# lists with `while read` loops, never mapfile.
set -euo pipefail

# Git with paths printed as they are: a quoted non-ASCII name would match no rule.
git() { command git -c core.quotePath=false "$@"; }

cd "$(git rev-parse --show-toplevel)"

violations=0

# report PATH LINE RULE EXCERPT
report() {
  local excerpt=$4
  excerpt=${excerpt#"${excerpt%%[![:space:]]*}"}
  printf '%s:%s: %s: %s\n' "$1" "$2" "$3" "${excerpt:0:120}"
  violations=$((violations + 1))
}

# report_hits RULE, reading `path:line:text` lines (grep -nH, git grep -n) from stdin
report_hits() {
  local hit path line
  while IFS= read -r hit; do
    path=${hit%%:*}
    hit=${hit#*:}
    line=${hit%%:*}
    report "$path" "$line" "$1" "${hit#*:}"
  done
}

# Tracked files and untracked ones that are not ignored, as long as they exist on disk. File
# lists are NUL-separated from here on, so every name is read whole.
files=()
while IFS= read -r -d '' f; do
  if [[ -f $f ]]; then files+=("$f"); fi
done < <(git ls-files -z --cached --others --exclude-standard | sort -zu)

# matching REGEX: the files whose path matches the extended regex, NUL-separated
matching() {
  local f
  for f in "${files[@]}"; do
    if [[ $f =~ $1 ]]; then printf '%s\0' "$f"; fi
  done
}

# 1. No tracked file is one the ignore rules ignore: only `git add -f` puts one there. This
#    catches a docs page .gitignore does not publish, anything under .claude/, NOTES.md and the
#    other internal names.
while IFS= read -r -d '' f; do
  report "$f" 1 ignored-tracked "tracked but ignored by .gitignore; untrack it (git rm --cached)"
done < <(git ls-files -z -ci --exclude-standard)

# 2. Markdown lives only at these paths, spelled `.md`. Specs, plans, notes and other internal
#    files go under .claude/ (ignored), never in the repo.
allowed='^(README|CONTRIBUTING|CHANGELOG)\.md$|^(.+/)?(AGENTS|CLAUDE)\.md$|^docs/[^/]+\.md$'
allowed+='|^(data|demo)/README\.md$|^\.github/ISSUE_TEMPLATE/[^/]+\.md$'
allowed+='|^\.github/pull_request_template\.md$'
markdown=()
while IFS= read -r -d '' f; do
  if [[ $f =~ $allowed ]]; then
    markdown+=("$f")
  else
    report "$f" 1 allowlist "not an allowed docs path; internal files go under .claude/"
  fi
done < <(matching '\.[Mm][Dd]$')

# 3. A CLAUDE.md holds only `@AGENTS.md`: the guide itself is AGENTS.md.
only_import='^[[:space:]]*@AGENTS\.md[[:space:]]*$'
while IFS= read -r -d '' f; do
  report_hits claude-md < <(grep -nHv -e '^[[:space:]]*$' -e "$only_import" -- "$f")
  grep -q -e "$only_import" -- "$f" || report "$f" 1 claude-md "missing @AGENTS.md"
done < <(matching '(^|/)CLAUDE\.md$')

# 4. Private data, in every text file but the language models and wordlists under data/.
# grep_text GIT-GREP-OPTIONS...: git grep -n over those files, untracked ones included
grep_text() {
  git grep -nI --untracked "$@" -- . ':(exclude)data/model-*.data' ':(exclude)data/wordlist-*.txt'
}
# Home directories (Linux /home, macOS /Users), except the placeholder user examples use. The
# name never ends in `.`: that is the end of a sentence.
placeholder='^/(home|Users)/user$'
while IFS= read -r hit; do
  [[ ${hit##*:} =~ $placeholder ]] || report_hits private-path <<<"$hit"
done < <(grep_text -oE '/(home/[a-z]|Users/[A-Za-z])([A-Za-z0-9_.-]*[A-Za-z0-9_-])?')
# Project checkouts under the home directory, written with a tilde or $HOME.
# shellcheck disable=SC2016 # the pattern matches a literal $HOME
report_hits private-path < <(grep_text -E '(~|\$HOME|\$\{HOME\})/Projects[/]')
report_hits private-link < <(grep_text -E 'claude\.ai($|[^A-Za-z0-9-])')
# Email addresses. Allowed: noreply ones (the github-actions bot in the workflows), the reserved
# example domains, and git@, the SSH user of a remote (git@github.com:owner/repo). Cargo.toml
# has no authors field, so it adds none.
email_ok='^git@|no-?reply|@example\.(com|org|net)$'
while IFS= read -r hit; do
  [[ ${hit#*:*:} =~ $email_ok ]] || report_hits private-email <<<"$hit"
done < <(grep_text -oE '[][A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}')

# 5. Wording. No em or en dashes (U+2014, U+2013) in any markdown file.
if ((${#markdown[@]})); then
  report_hits dash < <(grep -nHF -e $'\xe2\x80\x94' -e $'\xe2\x80\x93' -- "${markdown[@]}")
fi
# The user docs and the AGENTS.md files carry no test provenance and no fix history: those go
# in the commit. CHANGELOG.md is history by design, so it is not one of them.
docs_re='^(README|CONTRIBUTING)\.md$|^docs/[^/]+\.md$|^(data|demo)/README\.md$|^(.+/)?AGENTS\.md$'
docs=()
while IFS= read -r -d '' f; do docs+=("$f"); done < <(matching "$docs_re")

# grep_docs RULE GREP-OPTIONS...
grep_docs() {
  local rule=$1
  shift
  if ((${#docs[@]})); then
    report_hits "$rule" < <(grep -nH "$@" -- "${docs[@]}")
  fi
}
# "measured on" only as a measurement of the maintainer's, not "measured against your speed".
provenance='\(measured\)|measured on (a|my|our|his|this) |\btested (with|on|against)\b'
provenance+='|on my (machine|laptop)|mock model'
grep_docs provenance -iE "$provenance"
# Versions are this project's own (0.x): "before 1.88.0" names a Rust release. "Older versions"
# only of keybr-tui: an older terminal is a fact for the user.
history='\b(since|until|in|see|fixes|fixed|closes|issue|pr) #[0-9]+|\(#[0-9]+\)|/(issues|pull)/[0-9]+'
history+='|\b(since|until|as of|as in|before) v?0\.[0-9]+\.[0-9]+|\b(this|it) used to\b'
history+='|\bolder versions? of keybr-tui\b|\ban older keybr-tui\b'
grep_docs history -iE "$history"

# 6. Line budgets. Cut before adding.
# budget FILE LIMIT
budget() {
  local n
  n=$(($(wc -l <"$1"))) # BSD wc pads the count with spaces
  ((n <= $2)) || report "$1" $(($2 + 1)) budget "$n lines, the limit is $2"
}
while IFS= read -r -d '' f; do
  case $f in
  README.md) budget "$f" 220 ;;
  AGENTS.md) budget "$f" 150 ;;
  */AGENTS.md) budget "$f" 60 ;;
  docs/*.md) budget "$f" 300 ;;
  esac
done < <(matching '^(README\.md|(.+/)?AGENTS\.md|docs/[^/]+\.md)$')

if ((violations)); then
  echo "check-docs: $violations violation(s)"
  exit 1
fi
echo "check-docs: ok"
