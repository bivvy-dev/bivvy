#!/usr/bin/env bash
set -euo pipefail

# Remote workflow acceptance tests
# Requires: network access, bivvy-dev/shared-configs repo on GitHub

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BIVVY="${BIVVY:-$REPO_ROOT/target/debug/bivvy}"

if [ ! -f "$BIVVY" ]; then
  echo "Error: bivvy binary not found at $BIVVY"
  echo "Run 'cargo build' first, or set BIVVY=/path/to/bivvy"
  exit 1
fi

PASS=0
FAIL=0

pass() {
  echo "  PASS: $1"
  PASS=$((PASS + 1))
}

fail() {
  echo "  FAIL: $1"
  echo "        $2"
  FAIL=$((FAIL + 1))
}

echo "Running remote workflow acceptance tests..."
echo "Binary: $BIVVY"
echo ""

# Clear remote config cache to ensure fresh fetches
rm -rf ~/Library/Caches/bivvy/remote-configs/ 2>/dev/null || true
rm -rf ~/Library/Caches/bivvy/templates/ 2>/dev/null || true

# --- extends-basic ---
echo "=== extends-basic ==="
OUTPUT=$(cd "$SCRIPT_DIR/extends-basic" && "$BIVVY" list 2>&1)
if echo "$OUTPUT" | grep -q "check_git"; then
  pass "Base step 'check_git' appears in list"
else
  fail "Base step 'check_git' missing from list" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "check_curl"; then
  pass "Base step 'check_curl' appears in list"
else
  fail "Base step 'check_curl' missing from list" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "local_step"; then
  pass "Local step 'local_step' appears in list"
else
  fail "Local step 'local_step' missing from list" "$OUTPUT"
fi

# --- extends-override ---
echo ""
echo "=== extends-override ==="
OUTPUT=$(cd "$SCRIPT_DIR/extends-override" && "$BIVVY" list 2>&1)
if echo "$OUTPUT" | grep -q "echo 'Local override wins'"; then
  pass "Local step definition overrides base"
else
  fail "Local override not applied" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "check_curl"; then
  pass "Non-overridden base step preserved"
else
  fail "Non-overridden base step missing" "$OUTPUT"
fi

# --- extends-local-override ---
echo ""
echo "=== extends-local-override ==="
OUTPUT=$(cd "$SCRIPT_DIR/extends-local-override" && "$BIVVY" status 2>&1)
if echo "$OUTPUT" | grep -q "Local Override Name"; then
  pass "config.local.yml overrides app_name"
else
  fail "config.local.yml override not applied" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "check_git\|check_curl"; then
  pass "Base steps still present with local override"
else
  fail "Base steps missing with local override" "$OUTPUT"
fi

# --- extends-bad-url ---
echo ""
echo "=== extends-bad-url ==="
OUTPUT=$(cd "$SCRIPT_DIR/extends-bad-url" && "$BIVVY" lint 2>&1 || true)
if echo "$OUTPUT" | grep -q "404"; then
  pass "Unreachable URL produces 404 error"
else
  fail "Expected 404 error for bad URL" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "nonexistent-repo"; then
  pass "Error message includes the URL"
else
  fail "Error message missing URL context" "$OUTPUT"
fi

# --- template-sources ---
echo ""
echo "=== template-sources ==="
OUTPUT=$(cd "$SCRIPT_DIR/template-sources" && "$BIVVY" list 2>&1)
if echo "$OUTPUT" | grep -q "template: remote-hello"; then
  pass "Remote template 'remote-hello' resolved"
else
  fail "Remote template 'remote-hello' not resolved" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "template: custom-check"; then
  pass "Remote template 'custom-check' resolved"
else
  fail "Remote template 'custom-check' not resolved" "$OUTPUT"
fi

RUN_OUTPUT=$(cd "$SCRIPT_DIR/template-sources" && "$BIVVY" run --non-interactive --verbose 2>&1)
if echo "$RUN_OUTPUT" | grep -q "Hello from remote template"; then
  pass "Remote template step executed successfully"
else
  fail "Remote template step didn't execute" "$RUN_OUTPUT"
fi

# --- full-story ---
echo ""
echo "=== full-story ==="
OUTPUT=$(cd "$SCRIPT_DIR/full-story" && "$BIVVY" list 2>&1)
if echo "$OUTPUT" | grep -q "check_git" && echo "$OUTPUT" | grep -q "check_curl"; then
  pass "Extends base steps present"
else
  fail "Extends base steps missing" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "template: remote-hello"; then
  pass "Remote template step present"
else
  fail "Remote template step missing" "$OUTPUT"
fi
if echo "$OUTPUT" | grep -q "local_step"; then
  pass "Local step present"
else
  fail "Local step missing" "$OUTPUT"
fi

RUN_OUTPUT=$(cd "$SCRIPT_DIR/full-story" && "$BIVVY" run --non-interactive --verbose 2>&1)
if echo "$RUN_OUTPUT" | grep -q "Setup complete"; then
  pass "Full story run completed successfully"
else
  fail "Full story run didn't complete" "$RUN_OUTPUT"
fi

LINT_OUTPUT=$(cd "$SCRIPT_DIR/full-story" && "$BIVVY" lint 2>&1)
if echo "$LINT_OUTPUT" | grep -q "0 error"; then
  pass "Lint passes on merged config"
else
  fail "Lint failed on merged config" "$LINT_OUTPUT"
fi

STATUS_OUTPUT=$(cd "$SCRIPT_DIR/full-story" && "$BIVVY" status 2>&1)
if echo "$STATUS_OUTPUT" | grep -q "Full Story Test"; then
  pass "Status shows correct app name"
else
  fail "Status shows wrong app name" "$STATUS_OUTPUT"
fi

# --- Summary ---
echo ""
echo "================================"
echo "Results: $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
echo "All acceptance tests passed!"
