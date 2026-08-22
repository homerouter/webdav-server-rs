#!/usr/bin/env bash
#
# WebDAV Protocol & Server End-to-End Test Suite
#
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASSED_COUNT=0
FAILED_COUNT=0

BINARY="${1:-target/debug/webdav-server}"
if [ ! -f "$BINARY" ]; then
    if [ -f "target/release/webdav-server" ]; then
        BINARY="target/release/webdav-server"
    else
        echo -e "${RED}ERROR: Binary '$BINARY' not found. Please build with 'cargo build' first.${NC}"
        exit 1
    fi
fi

PORT="${PORT:-49180}"
BASE_URL="http://127.0.0.1:${PORT}"

# Setup temporary workspace
TEST_BASE_DIR="${TMPDIR:-/tmp}"
TEST_DIR="$(mktemp -d "${TEST_BASE_DIR}/webdav-e2e-XXXXXX" 2>/dev/null || mktemp -d -t webdav-e2e-XXXXXX 2>/dev/null || mktemp -d)"
PUBLIC_DIR="${TEST_DIR}/public"
SECURE_DIR="${TEST_DIR}/secure"
HTPASSWD_FILE="${TEST_DIR}/htpasswd"
CONFIG_FILE="${TEST_DIR}/config.toml"
SERVER_LOG="${TEST_DIR}/server.log"

mkdir -p "$PUBLIC_DIR" "$SECURE_DIR"

# Create htpasswd for user 'testuser' with password 'testpass123'
echo 'testuser:$6$1tiQyAMp5EEviJiu$kUPNGuCt31zOwpNaLTdiAUC0w7ZZWLzaqF9AjJUV5TkXF0WWBmTeZscuqUjguCSs/YTuc6aor7QMQj6ZW5WLh1' > "$HTPASSWD_FILE"

# Create configuration file
cat <<EOF > "$CONFIG_FILE"
[server]
  listen = [ "127.0.0.1:${PORT}" ]
  identification = "webdav-server-rs-e2e"

[htpasswd.testauth]
  htpasswd = "${HTPASSWD_FILE}"

[[location]]
  route = [ "/public/*path" ]
  methods = [ "webdav-rw" ]
  auth = "false"
  handler = "filesystem"
  directory = "${PUBLIC_DIR}"

[[location]]
  route = [ "/secure/*path" ]
  methods = [ "webdav-rw" ]
  auth-type = "htpasswd.testauth"
  auth = "true"
  handler = "filesystem"
  directory = "${SECURE_DIR}"
EOF

echo -e "${BLUE}====================================================${NC}"
echo -e "${BLUE}  Starting WebDAV End-to-End Test Suite             ${NC}"
echo -e "${BLUE}  Binary: $BINARY                                   ${NC}"
echo -e "${BLUE}  URL:    $BASE_URL                                 ${NC}"
echo -e "${BLUE}====================================================${NC}"

# Start webdav-server in background
"$BINARY" -c "$CONFIG_FILE" -D > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!

cleanup() {
    echo -e "\n${YELLOW}Cleaning up test server (PID $SERVER_PID)...${NC}"
    if kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT INT TERM

# Wait for server to start
echo -n "Waiting for server to become ready..."
READY=0
for i in {1..30}; do
    if curl -s --max-time 2 -o /dev/null -w "%{http_code}" "$BASE_URL/public/" >/dev/null 2>&1; then
        READY=1
        echo " Ready!"
        break
    fi
    sleep 0.2
    echo -n "."
done

if [ "$READY" -ne 1 ]; then
    echo -e "\n${RED}Server failed to start. Logs:${NC}"
    cat "$SERVER_LOG"
    exit 1
fi

assert_status() {
    local test_name="$1"
    local expected_status="$2"
    local actual_status="$3"

    if [ "$actual_status" = "$expected_status" ]; then
        echo -e "  [PASS] $test_name (HTTP $actual_status)"
        PASSED_COUNT=$((PASSED_COUNT + 1))
    else
        echo -e "  ${RED}[FAIL] $test_name (Expected HTTP $expected_status, got $actual_status)${NC}"
        FAILED_COUNT=$((FAILED_COUNT + 1))
    fi
}

assert_contains() {
    local test_name="$1"
    local expected="$2"
    local actual="$3"

    if [[ "$actual" == *"$expected"* ]]; then
        echo -e "  [PASS] $test_name (Matches '$expected')"
        PASSED_COUNT=$((PASSED_COUNT + 1))
    else
        echo -e "  ${RED}[FAIL] $test_name (Expected '$expected' in output)${NC}"
        echo -e "         Actual: $actual"
        FAILED_COUNT=$((FAILED_COUNT + 1))
    fi
}

echo -e "\n${YELLOW}--- 1. WebDAV Basic & Public Operations ---${NC}"

# Test 1: OPTIONS /public/
RESP=$(curl -s --max-time 5 -i -X OPTIONS "$BASE_URL/public/")
STATUS=$(echo "$RESP" | head -n 1 | awk '{print $2}')
assert_status "OPTIONS /public/" "200" "$STATUS"
LOWER_RESP=$(echo "$RESP" | tr '[:upper:]' '[:lower:]')
assert_contains "OPTIONS DAV header check" "dav:" "$LOWER_RESP"

# Test 2: MKCOL /public/testcol
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -X MKCOL "$BASE_URL/public/testcol")
assert_status "MKCOL /public/testcol" "201" "$STATUS"

# Test 3: PUT /public/testcol/sample.txt
PAYLOAD="Hello WebDAV World 2026"
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -X PUT --data "$PAYLOAD" "$BASE_URL/public/testcol/sample.txt")
assert_status "PUT /public/testcol/sample.txt" "201" "$STATUS"

# Test 4: GET /public/testcol/sample.txt
BODY=$(curl -s --max-time 5 -X GET "$BASE_URL/public/testcol/sample.txt")
assert_contains "GET /public/testcol/sample.txt body" "$PAYLOAD" "$BODY"

# Test 5: HEAD /public/testcol/sample.txt
RESP=$(curl -s --max-time 5 -I "$BASE_URL/public/testcol/sample.txt")
STATUS=$(echo "$RESP" | head -n 1 | awk '{print $2}')
assert_status "HEAD /public/testcol/sample.txt" "200" "$STATUS"
assert_contains "HEAD Content-Length" "content-length:" "$(echo "$RESP" | tr '[:upper:]' '[:lower:]')"

# Test 6: PROPFIND /public/testcol
PROPFIND_RESP=$(curl -s --max-time 5 -X PROPFIND -H "Depth: 1" "$BASE_URL/public/testcol")
assert_contains "PROPFIND multistatus XML" "multistatus" "$PROPFIND_RESP"
assert_contains "PROPFIND sample.txt listed" "sample.txt" "$PROPFIND_RESP"

# Test 7: LOCK & UNLOCK
echo -e "\n${YELLOW}--- 2. WebDAV LOCK & UNLOCK ---${NC}"
LOCK_XML='<?xml version="1.0" encoding="utf-8" ?><D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype><D:owner><D:href>testsuite</D:href></D:owner></D:lockinfo>'
LOCK_RESP=$(curl -s --max-time 5 -i -X LOCK -H "Timeout: Second-3600" -H "Content-Type: application/xml" -d "$LOCK_XML" "$BASE_URL/public/testcol/sample.txt")
STATUS=$(echo "$LOCK_RESP" | head -n 1 | awk '{print $2}')
assert_status "LOCK /public/testcol/sample.txt" "200" "$STATUS"

LOCK_TOKEN=$(echo "$LOCK_RESP" | grep -i '^Lock-Token:' | sed -e 's/^[Ll]ock-[Tt]oken:[[:space:]]*//' -e 's/[<>]//g' | tr -d '\r\n' || true)
if [ -n "$LOCK_TOKEN" ]; then
    echo -e "  [PASS] Lock token acquired: $LOCK_TOKEN"
    PASSED_COUNT=$((PASSED_COUNT + 1))

    # Test UNLOCK
    UNLOCK_STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -X UNLOCK -H "Lock-Token: <$LOCK_TOKEN>" "$BASE_URL/public/testcol/sample.txt")
    assert_status "UNLOCK /public/testcol/sample.txt (204)" "204" "$UNLOCK_STATUS"
else
    echo -e "  ${RED}[FAIL] Could not parse Lock-Token from response${NC}"
    FAILED_COUNT=$((FAILED_COUNT + 1))
fi

# Test 8: DELETE operations
echo -e "\n${YELLOW}--- 3. WebDAV DELETE Operations ---${NC}"
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/public/testcol/sample.txt")
assert_status "DELETE /public/testcol/sample.txt" "204" "$STATUS"

STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/public/testcol")
assert_status "DELETE /public/testcol" "204" "$STATUS"

# Test 9: Authentication (htpasswd)
echo -e "\n${YELLOW}--- 4. Authentication & Security Operations ---${NC}"

# Unauthenticated request -> 401
AUTH_STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" "$BASE_URL/secure/secret.txt")
assert_status "GET /secure/secret.txt without credentials (401)" "401" "$AUTH_STATUS"

# Wrong credentials -> 401
BAD_AUTH_STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "testuser:wrongpassword" "$BASE_URL/secure/secret.txt")
assert_status "GET /secure/secret.txt with wrong password (401)" "401" "$BAD_AUTH_STATUS"

# PUT with valid credentials -> 201
SECURE_PUT_STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "testuser:testpass123" -X PUT --data "Super secret data" "$BASE_URL/secure/secret.txt")
assert_status "PUT /secure/secret.txt with valid credentials (201)" "201" "$SECURE_PUT_STATUS"

# GET with valid credentials -> 200
SECURE_BODY=$(curl -s --max-time 5 -u "testuser:testpass123" -X GET "$BASE_URL/secure/secret.txt")
assert_contains "GET /secure/secret.txt body" "Super secret data" "$SECURE_BODY"

# DELETE with valid credentials -> 204
SECURE_DEL_STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "testuser:testpass123" -X DELETE "$BASE_URL/secure/secret.txt")
assert_status "DELETE /secure/secret.txt with valid credentials (204)" "204" "$SECURE_DEL_STATUS"

echo -e "\n${BLUE}====================================================${NC}"
echo -e "${BLUE}  WebDAV End-to-End Test Summary                   ${NC}"
echo -e "  ${GREEN}Passed: $PASSED_COUNT${NC}"
if [ "$FAILED_COUNT" -gt 0 ]; then
    echo -e "  ${RED}Failed: $FAILED_COUNT${NC}"
    echo -e "${BLUE}====================================================${NC}"
    exit 1
else
    echo -e "  ${GREEN}All tests passed successfully!${NC}"
    echo -e "${BLUE}====================================================${NC}"
    exit 0
fi
