#!/usr/bin/env bash
#
# WebDAV PAM Authentication End-to-End Test Suite for Alpine Linux
#
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASSED_COUNT=0
FAILED_COUNT=0

BINARY="${1:-target/release/webdav-server}"
if [ ! -f "$BINARY" ]; then
    if [ -f "target/debug/webdav-server" ]; then
        BINARY="target/debug/webdav-server"
    else
        echo -e "${RED}ERROR: Binary '$BINARY' not found. Please build first.${NC}"
        exit 1
    fi
fi

if [ "$(id -u)" -ne 0 ]; then
    echo -e "${RED}ERROR: This PAM test suite must run as root to verify /etc/shadow authentication.${NC}"
    exit 1
fi

PORT="${PORT:-49182}"
BASE_URL="http://127.0.0.1:${PORT}"
TEST_USER="testpamuser"
TEST_PASS="pampassword123"

echo -e "${BLUE}====================================================${NC}"
echo -e "${BLUE}  Starting Alpine Linux WebDAV PAM E2E Test Suite    ${NC}"
echo -e "${BLUE}  Binary: $BINARY                                   ${NC}"
echo -e "${BLUE}  URL:    $BASE_URL                                 ${NC}"
echo -e "${BLUE}====================================================${NC}"

# 1. Setup PAM service configuration for webdav
mkdir -p /etc/pam.d
cat << 'EOF' > /etc/pam.d/webdav
#%PAM-1.0
auth    required pam_unix.so
account required pam_unix.so
EOF

# 2. Setup Unix user in system
if ! id "$TEST_USER" >/dev/null 2>&1; then
    adduser -D -h "/home/$TEST_USER" -s /bin/sh "$TEST_USER"
fi
echo "$TEST_USER:$TEST_PASS" | chpasswd
USER_HOME=$(getent passwd "$TEST_USER" | cut -d: -f6)
mkdir -p "$USER_HOME"
chown -R "$TEST_USER:$TEST_USER" "$USER_HOME"

# 3. Setup temporary workspace & config
TEST_DIR="$(mktemp -d /tmp/webdav-pam-XXXXXX)"
CONFIG_FILE="${TEST_DIR}/config.toml"
SERVER_LOG="${TEST_DIR}/server.log"

cat <<EOF > "$CONFIG_FILE"
[server]
  listen = [ "127.0.0.1:${PORT}" ]
  identification = "webdav-server-alpine-pam"

[accounts]
  auth-type = "pam"
  acct-type = "unix"

[pam]
  service = "webdav"

[[location]]
  route = [ "/home/*path" ]
  methods = [ "webdav-rw" ]
  auth = "true"
  handler = "filesystem"
  directory = "${USER_HOME}"
EOF

# 4. Start webdav-server in background
"$BINARY" -c "$CONFIG_FILE" -D > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!

cleanup() {
    echo -e "\n${YELLOW}Cleaning up test server (PID $SERVER_PID)...${NC}"
    if kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_DIR"
    rm -f "${USER_HOME}/pam_sample.txt"
}
trap cleanup EXIT INT TERM

# Wait for server to become ready
echo -n "Waiting for server to become ready..."
READY=0
for i in {1..30}; do
    CODE=$(curl -s --max-time 2 -o /dev/null -w "%{http_code}" "$BASE_URL/home/" || true)
    if [ "$CODE" = "401" ] || [ "$CODE" = "200" ]; then
        READY=1
        echo " Ready! (HTTP $CODE)"
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

echo -e "\n${YELLOW}--- Testing PAM Authentication on Alpine Linux ---${NC}"

# Test 1: Request without credentials -> 401
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" "$BASE_URL/home/pam_sample.txt")
assert_status "GET without credentials (401)" "401" "$STATUS"

# Test 2: Request with wrong password -> 401
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "${TEST_USER}:wrongpassword" "$BASE_URL/home/pam_sample.txt")
assert_status "GET with wrong password (401)" "401" "$STATUS"

# Test 3: PUT with valid PAM credentials -> 201
PAYLOAD="Hello from Alpine PAM Test $(date +%s)"
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "${TEST_USER}:${TEST_PASS}" -X PUT --data "$PAYLOAD" "$BASE_URL/home/pam_sample.txt")
assert_status "PUT with valid PAM credentials (201)" "201" "$STATUS"

# Test 4: GET with valid PAM credentials -> 200
BODY=$(curl -s --max-time 5 -u "${TEST_USER}:${TEST_PASS}" -X GET "$BASE_URL/home/pam_sample.txt")
assert_contains "GET with valid PAM credentials body" "$PAYLOAD" "$BODY"

# Test 5: PROPFIND with valid PAM credentials -> 207
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "${TEST_USER}:${TEST_PASS}" -X PROPFIND -H "Depth: 1" "$BASE_URL/home/")
assert_status "PROPFIND with valid PAM credentials (207)" "207" "$STATUS"

# Test 6: DELETE with valid PAM credentials -> 204
STATUS=$(curl -s --max-time 5 -o /dev/null -w "%{http_code}" -u "${TEST_USER}:${TEST_PASS}" -X DELETE "$BASE_URL/home/pam_sample.txt")
assert_status "DELETE with valid PAM credentials (204)" "204" "$STATUS"

echo -e "\n${BLUE}====================================================${NC}"
echo -e "${BLUE}  Alpine Linux PAM Test Summary                     ${NC}"
echo -e "  ${GREEN}Passed: $PASSED_COUNT${NC}"
if [ "$FAILED_COUNT" -gt 0 ]; then
    echo -e "  ${RED}Failed: $FAILED_COUNT${NC}"
    echo -e "\n${RED}Server Log Output:${NC}"
    cat "$SERVER_LOG"
    echo -e "${BLUE}====================================================${NC}"
    exit 1
else
    echo -e "  ${GREEN}All PAM tests passed successfully!${NC}"
    echo -e "${BLUE}====================================================${NC}"
    exit 0
fi
