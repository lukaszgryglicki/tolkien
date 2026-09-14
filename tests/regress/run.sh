#!/bin/sh
# Regression check: translate every *.in and compare with the recorded *.out.
cd "$(dirname "$0")" || exit 1
BIN="${1:-../../target/x86_64-unknown-freebsd/release/tolkien}"
fail=0
for f in *.in; do
  b="${f%.in}"
  if ! "$BIN" < "$f" 2>/dev/null | diff -u "$b.out" - > /dev/null; then
    echo "FAIL $b"; "$BIN" < "$f" 2>/dev/null | diff -u "$b.out" -; fail=1
  fi
done
[ "$fail" = 0 ] && echo "all regressions pass"
exit $fail
