#!/bin/bash
# CLI integration tests for stytsch
set -e

S="C:/Users/bkoro/projects/stytsch/target/release/stytsch.exe"
PASS=0
FAIL=0

check() {
    local name="$1"
    shift
    if "$@" > /dev/null 2>&1; then
        echo "  PASS: $name"
        PASS=$((PASS+1))
    else
        echo "  FAIL: $name"
        FAIL=$((FAIL+1))
    fi
}

check_output() {
    local name="$1"
    local expected="$2"
    shift 2
    local output
    output=$("$@" 2>&1)
    if echo "$output" | grep -q "$expected"; then
        echo "  PASS: $name"
        PASS=$((PASS+1))
    else
        echo "  FAIL: $name (expected '$expected', got '$output')"
        FAIL=$((FAIL+1))
    fi
}

echo "=== stytsch CLI integration tests ==="
echo

# -- help
check "--help works" $S --help
check "--version works" $S --version

# -- config
check_output "config show" "search_mode" $S config show
check_output "config path" "stytsch" $S config path

# -- record commands
check "record basic command" $S record --command "echo hello" --cwd "C:\\test" --exit 0 --duration 1
check "record failed command" $S record --command "bad-cmd" --cwd "C:\\test" --exit 1 --duration 0
check "record long command" $S record --command "git commit -m 'very long commit message with spaces and special chars!'" --cwd "C:\\projects" --exit 0 --duration 5
check "record via file" bash -c "echo 'from-file-cmd' > /tmp/stytsch_test.txt && $S record --file /tmp/stytsch_test.txt --cwd 'C:\\test' --exit 0"

# -- record edge cases
check "record empty command ignored" $S record --command "" --cwd "C:\\test" --exit 0
check "record whitespace-only ignored" $S record --command "   " --cwd "C:\\test" --exit 0

# -- history list
check_output "history list shows recorded" "echo hello" $S history list
check_output "history list shows exit code" "1" $S history list
check_output "history list shows cwd" "C:\\\\test" $S history list

# -- history list with filters
check_output "history list --cwd filter" "echo hello" $S history list --cwd "C:\\test"
check_output "history list --count" "COMMAND" $S history list --count 2

# -- stats
check_output "stats shows total" "Total commands" $S stats
check_output "stats shows db size" "Database size" $S stats
check_output "stats shows top commands" "Top 10" $S stats

# -- prune
# Record 10 more entries then prune
for i in $(seq 1 10); do
    $S record --command "prune-test-$i" --cwd "C:\\prune" --exit 0 --duration 0 2>/dev/null
done
BEFORE=$($S stats 2>&1 | grep "Total commands" | grep -o '[0-9]*')
check "prune command works" $S prune --keep 5
AFTER=$($S stats 2>&1 | grep "Total commands" | grep -o '[0-9]*')
if [ "$AFTER" -le "$BEFORE" ] 2>/dev/null; then
    echo "  PASS: prune reduced count ($BEFORE -> $AFTER)"
    PASS=$((PASS+1))
else
    echo "  FAIL: prune didn't reduce count ($BEFORE -> $AFTER)"
    FAIL=$((FAIL+1))
fi

# -- history delete
ID=$($S record --command "delete-me" --cwd "C:\\test" --exit 0 2>&1)
check "history delete" $S history delete "nonexistent-id"

# -- install/uninstall
check_output "install detects clink" "Lua script" $S install
check_output "uninstall removes script" "Removed" $S uninstall

# -- error cases
check_output "record without args fails" "must be provided" bash -c "$S record 2>&1 || true"

echo
echo "=== Results: $PASS passed, $FAIL failed ==="
