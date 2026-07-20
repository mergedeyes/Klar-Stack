#!/usr/bin/env fish
# Klar API — complete test suite
# Usage: fish test.fish [path-to-test-image]
# Requires: Klar running on localhost:3000, and psql available locally (for reading email tokens directly from the DB)
#
# AUTH MODEL: the backend issues httpOnly cookies (klar_access_token,
# klar_refresh_token) via Set-Cookie on register/login/refresh — it no
# longer returns usable tokens in the JSON body (those fields are kept
# as empty strings for backward compatibility only). So every
# authenticated request here uses a per-user curl cookie jar (-b to
# send, -c to save) instead of an Authorization: Bearer header.

set BASE "http://localhost:3000"
# DATABASE_URL is read from .env (same directory as this script) so we can
# pull verification/reset tokens directly from the DB instead of relying on
# catching the email -- this project sends real mail via IONOS, not MailHog.
set SCRIPT_DIR (dirname (status -f))
set DATABASE_URL (grep -E '^DATABASE_URL=' $SCRIPT_DIR/.env | cut -d= -f2- | tr -d '"')
set PASSED 0
set FAILED 0
set FAILED_TESTS

set TEST_IMAGE (test (count $argv) -gt 0; and echo $argv[1]; or echo "$HOME/Downloads/test.png")

if not test -f $TEST_IMAGE
    echo "ERROR: Test image not found: $TEST_IMAGE"
    echo "Usage: fish test.fish /path/to/any/image.jpg"
    exit 1
end

function pass
    set -g PASSED (math $PASSED + 1)
    echo "  ✓ PASS"
end

function fail
    set -g FAILED (math $FAILED + 1)
    set -g FAILED_TESTS $FAILED_TESTS "$argv[1]"
    echo "  ✗ FAIL: $argv[1]"
end

function assert_status --argument-names test_name expected actual
    if test "$expected" = "$actual"
        pass
    else
        fail "$test_name (expected HTTP $expected, got $actual)"
    end
end

function assert_json_field --argument-names test_name json field
    set val (echo $json | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('$field','__MISSING__'))" 2>/dev/null)
    if test "$val" != "__MISSING__"; and test "$val" != "" ; and test "$val" != "None"
        pass
    else
        fail "$test_name (missing field: $field)"
    end
end

function assert_json_value --argument-names test_name json field expected
    set val (echo $json | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('$field','__MISSING__'))" 2>/dev/null)
    if test "$val" = "$expected"
        pass
    else
        fail "$test_name (expected $field=$expected, got $val)"
    end
end

function assert_json_nested --argument-names test_name json path expected
    set val (echo $json | python3 -c "
import sys,json
d=json.load(sys.stdin)
keys='$path'.split('.')
for k in keys:
    if isinstance(d,dict): d=d.get(k,'__MISSING__')
    else: d='__MISSING__'
print(d)" 2>/dev/null)
    if test "$val" = "$expected"
        pass
    else
        fail "$test_name (expected $path=$expected, got $val)"
    end
end

function assert_file_exists --argument-names test_name filepath
    if test -f $filepath
        pass
    else
        fail "$test_name (file missing: $filepath)"
    end
end

function assert_file_missing --argument-names test_name filepath
    if not test -f $filepath
        pass
    else
        fail "$test_name (file should be deleted: $filepath)"
    end
end

# Checks that a cookie jar contains a named cookie with a non-empty value
function assert_cookie_set --argument-names test_name jarfile cookie_name
    if test -f $jarfile
        set line (grep "	$cookie_name	" $jarfile 2>/dev/null | tail -n1)
        set val (echo $line | awk '{print $NF}')
        if test -n "$val"
            pass
        else
            fail "$test_name (cookie $cookie_name not set or empty)"
        end
    else
        fail "$test_name (no cookie jar)"
    end
end

# Pulls the most recent unused token for a given email + token_type
# (verification / password_reset) straight from the database.
function get_email_token --argument-names email token_type
    psql "$DATABASE_URL" -tAc "
        SELECT t.token FROM email_tokens t
        JOIN users u ON u.id = t.user_id
        WHERE u.email = '$email'
          AND t.token_type = '$token_type'
          AND t.used_at IS NULL
        ORDER BY t.created_at DESC
        LIMIT 1
    " 2>/dev/null | string trim
end


echo "╔══════════════════════════════════════════╗"
echo "║        Klar API — Full Test Suite        ║"
echo "╚══════════════════════════════════════════╝"
echo "Image: $TEST_IMAGE"
echo ""

set SUFFIX (random 10000 99999)
set JAN_USER "jan_$SUFFIX"
set ANNA_USER "anna_$SUFFIX"
set JAN_EMAIL "$JAN_USER@example.com"
set ANNA_EMAIL "$ANNA_USER@example.com"
set PASSWORD "sicheresPasswort123"

set JAN_JAR (mktemp)
set ANNA_JAR (mktemp)

# ══════════════════════════════════════════
# AUTH — Registration
# ══════════════════════════════════════════

echo "Running test: Register jan"
set JAN_RESPONSE (curl -s -c $JAN_JAR -X POST $BASE/auth/register \
    -H "Content-Type: application/json" \
    -d "{\"username\": \"$JAN_USER\", \"email\": \"$JAN_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Register jan username" "$JAN_RESPONSE" "user.username" "$JAN_USER"

echo "Running test: Register sets auth cookies"
assert_cookie_set "Register sets access cookie" "$JAN_JAR" "klar_access_token"
assert_cookie_set "Register sets refresh cookie" "$JAN_JAR" "klar_refresh_token"

echo "Running test: Register returns correct username"
assert_json_nested "Register username" "$JAN_RESPONSE" "user.username" "$JAN_USER"

echo "Running test: New user email_verified is false"
assert_json_nested "email_verified is false" "$JAN_RESPONSE" "user.email_verified" "False"

# Grab jan's verification token NOW, before registering anna
echo "Running test: Verification email sent for jan"
set JAN_VERIFY_TOKEN (get_email_token $JAN_EMAIL verification)
if test -n "$JAN_VERIFY_TOKEN"
    pass
else
    fail "Verification email sent for jan (no token in DB)"
end

echo "Running test: Register anna"
set ANNA_RESPONSE (curl -s -c $ANNA_JAR -X POST $BASE/auth/register \
    -H "Content-Type: application/json" \
    -d "{\"username\": \"$ANNA_USER\", \"email\": \"$ANNA_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Register anna username" "$ANNA_RESPONSE" "user.username" "$ANNA_USER"
assert_cookie_set "Register anna sets access cookie" "$ANNA_JAR" "klar_access_token"

echo "Running test: Duplicate registration rejected"
set DUP_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/register \
    -H "Content-Type: application/json" \
    -d "{\"username\": \"$JAN_USER\", \"email\": \"$JAN_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_status "Duplicate registration rejected" "409" "$DUP_STATUS"

echo "Running test: Empty username rejected"
set EMPTY_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/register \
    -H "Content-Type: application/json" \
    -d '{"username": "", "email": "x@x.de", "password": "12345678"}')
assert_status "Empty username rejected" "400" "$EMPTY_STATUS"

echo "Running test: Short password rejected"
set SHORT_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/register \
    -H "Content-Type: application/json" \
    -d '{"username": "shortpw", "email": "short@x.de", "password": "1234"}')
assert_status "Short password rejected" "400" "$SHORT_STATUS"

# ══════════════════════════════════════════
# AUTH — Email verification
# ══════════════════════════════════════════

echo "Running test: Verify jan's email"
if test -n "$JAN_VERIFY_TOKEN"
    set VERIFY_RESPONSE (curl -s "$BASE/auth/verify?token=$JAN_VERIFY_TOKEN")
    assert_json_value "Verify email" "$VERIFY_RESPONSE" "message" "Email verified successfully"
else
    fail "Verify email (no token)"
end

echo "Running test: email_verified is now true"
set ME_CHECK (curl -s $BASE/users/me -b $JAN_JAR)
assert_json_value "email_verified true" "$ME_CHECK" "email_verified" "True"

echo "Running test: Resend verification for already verified user fails"
set RESEND_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/resend-verification -b $JAN_JAR)
assert_status "Resend for verified user" "400" "$RESEND_STATUS"

echo "Running test: Used verification token rejected"
if test -n "$JAN_VERIFY_TOKEN"
    set REUSE_STATUS (curl -s -o /dev/null -w "%{http_code}" "$BASE/auth/verify?token=$JAN_VERIFY_TOKEN")
    assert_status "Used verification token" "400" "$REUSE_STATUS"
else
    fail "Used verification token (no token)"
end

echo "Running test: Invalid verification token rejected"
set BAD_VERIFY (curl -s -o /dev/null -w "%{http_code}" "$BASE/auth/verify?token=totally_invalid")
assert_status "Invalid verification token" "400" "$BAD_VERIFY"

# ══════════════════════════════════════════
# AUTH — Login
# ══════════════════════════════════════════

echo "Running test: Login jan"
set LOGIN_RESPONSE (curl -s -c $JAN_JAR -b $JAN_JAR -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Login username" "$LOGIN_RESPONSE" "user.username" "$JAN_USER"
assert_cookie_set "Login sets access cookie" "$JAN_JAR" "klar_access_token"

echo "Running test: Login with wrong password"
set BAD_LOGIN (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"wrongpassword\"}")
assert_status "Wrong password" "400" "$BAD_LOGIN"

echo "Running test: Login with nonexistent email"
set NO_EMAIL (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d '{"email": "nobody@nowhere.de", "password": "whatever123"}')
assert_status "Nonexistent email" "400" "$NO_EMAIL"

# ══════════════════════════════════════════
# AUTH — Refresh tokens
# ══════════════════════════════════════════

echo "Running test: Refresh token returns fresh cookies"
# Snapshot the pre-refresh jar so we can replay the now-stale refresh cookie later
set JAN_JAR_STALE (mktemp)
cp $JAN_JAR $JAN_JAR_STALE
set REFRESH_RESPONSE (curl -s -c $JAN_JAR -b $JAN_JAR -X POST $BASE/auth/refresh)
assert_cookie_set "Refresh rotates access cookie" "$JAN_JAR" "klar_access_token"

echo "Running test: Old (rotated-out) refresh cookie rejected"
set OLD_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/refresh -b $JAN_JAR_STALE)
assert_status "Old refresh token" "401" "$OLD_STATUS"

echo "Running test: Invalid refresh token rejected"
set FAKE_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/refresh \
    -H "Cookie: klar_refresh_token=totally_fake_12345")
assert_status "Invalid refresh token" "401" "$FAKE_STATUS"

echo "Running test: Refresh without any cookie rejected"
set NOCOOKIE_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/refresh)
assert_status "Refresh without cookie" "401" "$NOCOOKIE_STATUS"

# ══════════════════════════════════════════
# AUTH — Logout
# ══════════════════════════════════════════

echo "Running test: Logout"
# Login to a sacrificial jar so we don't disturb JAN_JAR's active session
set LOGOUT_JAR (mktemp)
curl -s -c $LOGOUT_JAR -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"$PASSWORD\"}" > /dev/null

set LOGOUT_RESPONSE (curl -s -c $LOGOUT_JAR -b $LOGOUT_JAR -X POST $BASE/auth/logout)
assert_json_value "Logout message" "$LOGOUT_RESPONSE" "message" "Logged out successfully"

echo "Running test: Refresh after logout fails"
set POST_LOGOUT (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/refresh -b $LOGOUT_JAR)
assert_status "Refresh after logout" "401" "$POST_LOGOUT"

# ══════════════════════════════════════════
# AUTH — Password reset
# ══════════════════════════════════════════

echo "Running test: Forgot password sends email"
set FORGOT_RESPONSE (curl -s -X POST $BASE/auth/forgot-password \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\"}")
assert_json_field "Forgot password message" "$FORGOT_RESPONSE" "message"

set RESET_TOKEN (get_email_token $JAN_EMAIL password_reset)
if test -n "$RESET_TOKEN"
    pass
else
    fail "Password reset email sent (no token in DB)"
end

echo "Running test: Forgot password nonexistent email (no leak)"
set FORGOT_FAKE (curl -s -X POST $BASE/auth/forgot-password \
    -H "Content-Type: application/json" \
    -d '{"email": "nobody@nowhere.de"}')
assert_json_field "No email leak" "$FORGOT_FAKE" "message"

echo "Running test: Reset password with valid token"
set NEW_PW "neuesPasswort456"
if test -n "$RESET_TOKEN"
    set RESET_RESPONSE (curl -s -X POST $BASE/auth/reset-password \
        -H "Content-Type: application/json" \
        -d "{\"token\": \"$RESET_TOKEN\", \"new_password\": \"$NEW_PW\"}")
    assert_json_field "Reset password" "$RESET_RESPONSE" "message"
else
    fail "Reset password (no token)"
end

echo "Running test: Login with new password"
set NEW_LOGIN (curl -s -c $JAN_JAR -b $JAN_JAR -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"$NEW_PW\"}")
assert_json_nested "Login new password" "$NEW_LOGIN" "user.username" "$JAN_USER"
assert_cookie_set "Login new password sets cookie" "$JAN_JAR" "klar_access_token"

echo "Running test: Old password no longer works"
set OLD_PW_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_status "Old password fails" "400" "$OLD_PW_STATUS"

echo "Running test: Used reset token rejected"
if test -n "$RESET_TOKEN"
    set REUSE_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/reset-password \
        -H "Content-Type: application/json" \
        -d "{\"token\": \"$RESET_TOKEN\", \"new_password\": \"another123\"}")
    assert_status "Used reset token" "400" "$REUSE_STATUS"
else
    fail "Used reset token (no token)"
end

echo "Running test: Reset with short password rejected"
set SHORT_RESET (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/reset-password \
    -H "Content-Type: application/json" \
    -d '{"token": "fake", "new_password": "123"}')
assert_status "Short reset password" "400" "$SHORT_RESET"

# Update password var for rest of tests
set PASSWORD "$NEW_PW"

# ══════════════════════════════════════════
# AUTH — Protected routes
# ══════════════════════════════════════════

echo "Running test: /users/me authenticated"
set ME (curl -s $BASE/users/me -b $JAN_JAR)
assert_json_value "/users/me username" "$ME" "username" "$JAN_USER"

echo "Running test: /users/me unauthenticated"
set UNAUTH (curl -s -o /dev/null -w "%{http_code}" $BASE/users/me)
assert_status "/users/me unauth" "401" "$UNAUTH"

echo "Running test: /users/me invalid token"
set BAD_TOKEN (curl -s -o /dev/null -w "%{http_code}" $BASE/users/me \
    -H "Authorization: Bearer totally.invalid.token")
assert_status "Invalid token" "401" "$BAD_TOKEN"

# ══════════════════════════════════════════
# PROFILE — Editing
# ══════════════════════════════════════════

echo "Running test: Update display name and bio"
set PROFILE (curl -s -X PATCH $BASE/users/me \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d '{"display_name": "Jan Kansen", "bio": "Building Klar."}')
assert_json_value "Update display_name" "$PROFILE" "display_name" "Jan Kansen"
assert_json_value "Update bio" "$PROFILE" "bio" "Building Klar."

echo "Running test: Update only bio (display_name unchanged)"
set BIO_ONLY (curl -s -X PATCH $BASE/users/me \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d '{"bio": "New bio only"}')
assert_json_value "Bio updated" "$BIO_ONLY" "bio" "New bio only"
assert_json_value "display_name unchanged" "$BIO_ONLY" "display_name" "Jan Kansen"

echo "Running test: Bio too long rejected"
set LONG_BIO (python3 -c "print('x' * 501)")
set LONG_BIO_STATUS (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d "{\"bio\": \"$LONG_BIO\"}")
assert_status "Bio too long" "400" "$LONG_BIO_STATUS"

echo "Running test: Upload avatar"
set AVATAR_RESPONSE (curl -s -X POST $BASE/users/me/avatar \
    -b $JAN_JAR \
    -F "avatar=@$TEST_IMAGE")
assert_json_field "Avatar returns avatar_url" "$AVATAR_RESPONSE" "avatar_url"

echo "Running test: Avatar served via HTTP"
set AVATAR_URL (echo $AVATAR_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin).get('avatar_url',''))" 2>/dev/null)
if test -n "$AVATAR_URL"; and test "$AVATAR_URL" != "None"
    set AVATAR_HTTP (curl -s -o /dev/null -w "%{http_code}" $BASE$AVATAR_URL)
    assert_status "Avatar HTTP" "200" "$AVATAR_HTTP"
else
    fail "Avatar served via HTTP (no URL)"
end

echo "Running test: Public profile shows updated fields"
set PUBLIC (curl -s $BASE/users/$JAN_USER)
assert_json_value "Public display_name" "$PUBLIC" "display_name" "Jan Kansen"
assert_json_field "Public avatar_url" "$PUBLIC" "avatar_url"

# ══════════════════════════════════════════
# POSTS — Text posts
# ══════════════════════════════════════════

echo "Running test: Create text post"
set POST_RESPONSE (curl -s -X POST $BASE/posts \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d '{"caption": "Erster Post!"}')
set POST_ID (echo $POST_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
assert_json_field "Create post" "$POST_RESPONSE" "id"

echo "Running test: Get post"
if test -n "$POST_ID"
    set GET_POST (curl -s $BASE/posts/$POST_ID)
    assert_json_value "Get post caption" "$GET_POST" "caption" "Erster Post!"
else
    fail "Get post (no post ID)"
end

echo "Running test: Edit post"
if test -n "$POST_ID"
    set EDIT_POST (curl -s -X PATCH $BASE/posts/$POST_ID \
        -H "Content-Type: application/json" \
        -b $JAN_JAR \
        -d '{"caption": "Bearbeitet!"}')
    assert_json_value "Edit caption" "$EDIT_POST" "caption" "Bearbeitet!"

    echo "Running test: Edit post sets edited_at"
    set EDIT_AT (echo $EDIT_POST | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if d.get('edited_at') else 'no')" 2>/dev/null)
    if test "$EDIT_AT" = "yes"; pass; else; fail "Edit sets edited_at"; end
else
    fail "Edit post (no post ID)"
    fail "Edit sets edited_at (no post ID)"
end

echo "Running test: Post not found"
set NF (curl -s -o /dev/null -w "%{http_code}" $BASE/posts/00000000-0000-0000-0000-000000000000)
assert_status "Post not found" "404" "$NF"

echo "Running test: Create post without auth"
set NOAUTH_POST (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts \
    -H "Content-Type: application/json" \
    -d '{"caption": "nope"}')
assert_status "Post without auth" "401" "$NOAUTH_POST"

# ══════════════════════════════════════════
# POSTS — Media upload
# ══════════════════════════════════════════

echo "Running test: Upload photo post"
set PHOTO_RESPONSE (curl -s -X POST $BASE/posts/upload \
    -b $JAN_JAR \
    -F "caption=Foto Post!" \
    -F "image=@$TEST_IMAGE")
set PHOTO_POST_ID (echo $PHOTO_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['post']['id'])" 2>/dev/null)
if test -n "$PHOTO_POST_ID"
    pass
else
    fail "Upload photo post (no post ID)"
end

echo "Running test: Upload returns media URLs"
set THUMB_URL (echo $PHOTO_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['media'][0]['thumb_url'])" 2>/dev/null)
set MEDIUM_URL (echo $PHOTO_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['media'][0]['medium_url'])" 2>/dev/null)
set FULL_URL (echo $PHOTO_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['media'][0]['full_url'])" 2>/dev/null)
if test -n "$THUMB_URL"; and test -n "$MEDIUM_URL"; and test -n "$FULL_URL"
    pass
else
    fail "Upload returns media URLs"
end

set THUMB_FILE (string replace "/media/" "uploads/" -- $THUMB_URL)
set MEDIUM_FILE (string replace "/media/" "uploads/" -- $MEDIUM_URL)
set FULL_FILE (string replace "/media/" "uploads/" -- $FULL_URL)

echo "Running test: Media files exist on disk"
if test -n "$THUMB_FILE"; and test "$THUMB_FILE" != "uploads/"
    assert_file_exists "Thumb on disk" "$THUMB_FILE"
    assert_file_exists "Medium on disk" "$MEDIUM_FILE"
    assert_file_exists "Full on disk" "$FULL_FILE"
else
    fail "Thumb on disk (no path)"; fail "Medium on disk (no path)"; fail "Full on disk (no path)"
end

echo "Running test: Media served via HTTP"
if test -n "$THUMB_URL"
    assert_status "Thumb HTTP" "200" (curl -s -o /dev/null -w "%{http_code}" $BASE$THUMB_URL)
    assert_status "Medium HTTP" "200" (curl -s -o /dev/null -w "%{http_code}" $BASE$MEDIUM_URL)
    assert_status "Full HTTP" "200" (curl -s -o /dev/null -w "%{http_code}" $BASE$FULL_URL)
else
    fail "Thumb HTTP (no URL)"; fail "Medium HTTP (no URL)"; fail "Full HTTP (no URL)"
end

echo "Running test: Get post media via API"
if test -n "$PHOTO_POST_ID"
    set MEDIA_API (curl -s $BASE/posts/$PHOTO_POST_ID/media)
    set MEDIA_COUNT (echo $MEDIA_API | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
    if test "$MEDIA_COUNT" = "1"; pass; else; fail "Post media count (expected 1, got $MEDIA_COUNT)"; end
else
    fail "Post media API (no post ID)"
end

echo "Running test: Upload without auth"
set NOAUTH_UPLOAD (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/upload \
    -F "caption=nope" \
    -F "image=@$TEST_IMAGE")
assert_status "Upload without auth" "401" "$NOAUTH_UPLOAD"

# ══════════════════════════════════════════
# FOLLOW SYSTEM
# ══════════════════════════════════════════

echo "Running test: Anna follows jan"
set FOLLOW (curl -s -X POST $BASE/users/$JAN_USER/follow -b $ANNA_JAR)
assert_json_field "Anna follows jan" "$FOLLOW" "message"

echo "Running test: Self-follow rejected"
set SELF (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$ANNA_USER/follow -b $ANNA_JAR)
assert_status "Self-follow" "400" "$SELF"

echo "Running test: Follow nonexistent user"
set GHOST (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/ghost_xyz/follow -b $ANNA_JAR)
assert_status "Follow nonexistent" "404" "$GHOST"

echo "Running test: Duplicate follow (idempotent)"
set DUP_FOLLOW (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$JAN_USER/follow -b $ANNA_JAR)
assert_status "Duplicate follow" "201" "$DUP_FOLLOW"

echo "Running test: Followers list"
set FOLLOWERS (curl -s $BASE/users/$JAN_USER/followers)
set F_COUNT (echo $FOLLOWERS | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$F_COUNT" = "1"; pass; else; fail "Followers (expected 1, got $F_COUNT)"; end

echo "Running test: Following list"
set FOLLOWING (curl -s $BASE/users/$ANNA_USER/following)
set FG_COUNT (echo $FOLLOWING | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$FG_COUNT" = "1"; pass; else; fail "Following (expected 1, got $FG_COUNT)"; end

echo "Running test: Following increments following_count"
set ANNA_STATS_1 (curl -s $BASE/users/$ANNA_USER/stats)
assert_json_value "Anna following_count after follow" "$ANNA_STATS_1" "following" "1"

# ══════════════════════════════════════════
# FAN-OUT (feed_items) — fan-out-on-write feed
# ══════════════════════════════════════════
# feed_items is populated at post-creation time (fan-out to current
# followers) and backfilled when a NEW follow happens (copying the
# followee's existing posts in), so a follower always sees full history
# immediately, not just posts made after they followed. This section
# tests exactly that backfill + the cleanup on unfollow, using a fresh
# user so it doesn't disturb jan/anna's state used elsewhere.

set FIONA_USER "fiona_$SUFFIX"
set FIONA_JAR (mktemp)

echo "Running test: Register fiona"
set FIONA_RESPONSE (curl -s -c $FIONA_JAR -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \"$FIONA_USER\", \"email\": \"$FIONA_USER@example.com\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Register fiona username" "$FIONA_RESPONSE" "user.username" "$FIONA_USER"

echo "Running test: New follower's feed is backfilled with existing posts"
if test -n "$POST_ID"
    curl -s -X POST $BASE/users/$JAN_USER/follow -b $FIONA_JAR > /dev/null
    set FIONA_FEED (curl -s $BASE/feed -b $FIONA_JAR)
    set FIONA_FEED_HAS_POST (echo $FIONA_FEED | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if any(p['id']=='$POST_ID' for p in d) else 'no')" 2>/dev/null)
    if test "$FIONA_FEED_HAS_POST" = "yes"; pass; else; fail "Feed backfill on follow"; end
else
    fail "Feed backfill on follow (no post ID)"
end

echo "Running test: Feed is strictly chronological (not reordered)"
if test -n "$POST_ID"
    set FIONA_FEED_ORDER (echo $FIONA_FEED | python3 -c "
import sys, json
d = json.load(sys.stdin)
dates = [p['created_at'] for p in d]
print('yes' if dates == sorted(dates, reverse=True) else 'no')
" 2>/dev/null)
    if test "$FIONA_FEED_ORDER" = "yes"; pass; else; fail "Feed chronological order"; end
else
    fail "Feed chronological order (no post ID)"
end

echo "Running test: Unfollow cleans up feed_items"
if test -n "$POST_ID"
    curl -s -X DELETE $BASE/users/$JAN_USER/follow -b $FIONA_JAR > /dev/null
    set FIONA_FEED_AFTER (curl -s $BASE/feed -b $FIONA_JAR)
    set FIONA_FEED_STILL_HAS_POST (echo $FIONA_FEED_AFTER | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if any(p['id']=='$POST_ID' for p in d) else 'no')" 2>/dev/null)
    if test "$FIONA_FEED_STILL_HAS_POST" = "no"; pass; else; fail "Feed cleanup on unfollow"; end
else
    fail "Feed cleanup on unfollow (no post ID)"
end

rm -f $FIONA_JAR 2>/dev/null

# ══════════════════════════════════════════
# FEED
# ══════════════════════════════════════════

echo "Running test: Anna's feed shows jan's posts"
set FEED (curl -s $BASE/feed -b $ANNA_JAR)
set FEED_COUNT (echo $FEED | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$FEED_COUNT" -gt 0 2>/dev/null; pass; else; fail "Anna's feed (empty)"; end

echo "Running test: Feed without auth"
set FEED_NOAUTH (curl -s -o /dev/null -w "%{http_code}" $BASE/feed)
assert_status "Feed without auth" "401" "$FEED_NOAUTH"

echo "Running test: User posts endpoint"
set UP (curl -s $BASE/users/$JAN_USER/posts)
set UP_COUNT (echo $UP | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$UP_COUNT" -gt 0 2>/dev/null; pass; else; fail "User posts (empty)"; end

echo "Running test: Discovery feed returns posts"
set DISCOVERY (curl -s "$BASE/feed/discovery?limit=10")
set DISC_COUNT (echo $DISCOVERY | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']))" 2>/dev/null)
if test "$DISC_COUNT" -gt 0 2>/dev/null; pass; else; fail "Discovery feed (empty)"; end

echo "Running test: Discovery feed post has caption field (not legacy 'content')"
set DISC_CAPTION_FIELD (echo $DISCOVERY | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print('yes' if d and 'caption' in d[0] else 'no')" 2>/dev/null)
if test "$DISC_CAPTION_FIELD" = "yes"; pass; else; fail "Discovery feed caption field"; end

echo "Running test: Discovery feed pagination cursor"
set DISC_CURSOR_TIME (echo $DISCOVERY | python3 -c "import sys,json; d=json.load(sys.stdin); c=d.get('next_cursor'); print(c['time'] if c else '')" 2>/dev/null)
set DISC_CURSOR_ID (echo $DISCOVERY | python3 -c "import sys,json; d=json.load(sys.stdin); c=d.get('next_cursor'); print(c['id'] if c else '')" 2>/dev/null)
if test -n "$DISC_CURSOR_TIME"; and test -n "$DISC_CURSOR_ID"
    set DISCOVERY_PAGE2 (curl -s "$BASE/feed/discovery?limit=10&cursor_time=$DISC_CURSOR_TIME&cursor_id=$DISC_CURSOR_ID")
    set DISC_PAGE2_STATUS (echo $DISCOVERY_PAGE2 | python3 -c "import sys,json; d=json.load(sys.stdin); print('ok' if 'data' in d else 'bad')" 2>/dev/null)
    if test "$DISC_PAGE2_STATUS" = "ok"; pass; else; fail "Discovery feed pagination"; end
else
    # Fewer posts than the page size — no next page, which is a valid end-state
    pass
end

# ══════════════════════════════════════════
# LIKES
# ══════════════════════════════════════════

echo "Running test: Like a post"
if test -n "$PHOTO_POST_ID"
    set LIKE (curl -s -X POST $BASE/posts/$PHOTO_POST_ID/like -b $ANNA_JAR)
    assert_json_value "Like" "$LIKE" "liked" "True"
    assert_json_value "Like count 1" "$LIKE" "like_count" "1"
else
    fail "Like (no post ID)"; fail "Like count (no post ID)"
end

echo "Running test: Get likes"
if test -n "$PHOTO_POST_ID"
    set LIKES (curl -s $BASE/posts/$PHOTO_POST_ID/likes)
    assert_json_value "Get like count" "$LIKES" "like_count" "1"
else
    fail "Get likes (no post ID)"
end

echo "Running test: Unlike (toggle)"
if test -n "$PHOTO_POST_ID"
    set UNLIKE (curl -s -X POST $BASE/posts/$PHOTO_POST_ID/like -b $ANNA_JAR)
    assert_json_value "Unlike" "$UNLIKE" "liked" "False"
    assert_json_value "Unlike count 0" "$UNLIKE" "like_count" "0"
else
    fail "Unlike (no post ID)"; fail "Unlike count (no post ID)"
end

echo "Running test: Like without auth"
if test -n "$PHOTO_POST_ID"
    set LIKE_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/$PHOTO_POST_ID/like)
    assert_status "Like without auth" "401" "$LIKE_NOAUTH"
else
    fail "Like without auth (no post ID)"
end

# ══════════════════════════════════════════
# NOTIFICATIONS
# ══════════════════════════════════════════
# Anna's like on jan's photo post above (even though later un-liked) should
# have created a persistent post_like notification for jan.

echo "Running test: Notifications list for jan includes post_like"
set NOTIFS (curl -s $BASE/notifications -b $JAN_JAR)
set NOTIF_TYPE (echo $NOTIFS | python3 -c "import sys,json; d=json.load(sys.stdin); print(d[0]['type_name'] if d else 'EMPTY')" 2>/dev/null)
if test "$NOTIF_TYPE" = "post_like"; pass; else; fail "Notifications post_like (got: $NOTIF_TYPE)"; end

echo "Running test: Notification actor is anna"
set NOTIF_ACTOR (echo $NOTIFS | python3 -c "import sys,json; d=json.load(sys.stdin); print(d[0]['actor']['username'] if d else '')" 2>/dev/null)
if test "$NOTIF_ACTOR" = "$ANNA_USER"; pass; else; fail "Notification actor (got: $NOTIF_ACTOR)"; end

echo "Running test: Notification starts unread"
set NOTIF_UNREAD (echo $NOTIFS | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if d and not d[0]['is_read'] else 'no')" 2>/dev/null)
if test "$NOTIF_UNREAD" = "yes"; pass; else; fail "Notification starts unread"; end

echo "Running test: Notifications without auth"
set NOTIF_NOAUTH (curl -s -o /dev/null -w "%{http_code}" $BASE/notifications)
assert_status "Notifications without auth" "401" "$NOTIF_NOAUTH"

echo "Running test: Mark notifications as read"
set MARK_READ (curl -s -X PATCH $BASE/notifications/read -b $JAN_JAR)
assert_json_field "Mark read response" "$MARK_READ" "message"

echo "Running test: Notification is now read"
set NOTIFS_AFTER (curl -s $BASE/notifications -b $JAN_JAR)
set NOTIF_READ (echo $NOTIFS_AFTER | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if d and d[0]['is_read'] else 'no')" 2>/dev/null)
if test "$NOTIF_READ" = "yes"; pass; else; fail "Notification marked read"; end

echo "Running test: Mark read without auth"
set MARK_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/notifications/read)
assert_status "Mark read without auth" "401" "$MARK_NOAUTH"

echo "Running test: Repeated like/unlike doesn't create duplicate notifications"
if test -n "$PHOTO_POST_ID"
    curl -s -X POST $BASE/posts/$PHOTO_POST_ID/like -b $ANNA_JAR > /dev/null
    curl -s -X POST $BASE/posts/$PHOTO_POST_ID/like -b $ANNA_JAR > /dev/null
    curl -s -X POST $BASE/posts/$PHOTO_POST_ID/like -b $ANNA_JAR > /dev/null
    set NOTIFS_DEDUP (curl -s $BASE/notifications -b $JAN_JAR)
    set NOTIF_DEDUP_COUNT (echo $NOTIFS_DEDUP | python3 -c "import sys,json; d=json.load(sys.stdin); print(len([n for n in d if n['type_name']=='post_like']))" 2>/dev/null)
    if test "$NOTIF_DEDUP_COUNT" = "1"; pass; else; fail "Notification dedup (expected 1 post_like row, got $NOTIF_DEDUP_COUNT)"; end
else
    fail "Notification dedup (no post ID)"
end

# ══════════════════════════════════════════
# COMMENTS
# ══════════════════════════════════════════

echo "Running test: Create comment"
if test -n "$PHOTO_POST_ID"
    set COMMENT (curl -s -X POST $BASE/posts/$PHOTO_POST_ID/comments \
        -H "Content-Type: application/json" \
        -b $ANNA_JAR \
        -d '{"body": "Tolles Foto!"}')
    set COMMENT_ID (echo $COMMENT | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
    assert_json_field "Create comment" "$COMMENT" "id"
else
    fail "Create comment (no post ID)"
end

echo "Running test: Reply to comment"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set REPLY (curl -s -X POST $BASE/posts/$PHOTO_POST_ID/comments \
        -H "Content-Type: application/json" \
        -b $JAN_JAR \
        -d "{\"body\": \"Danke!\", \"parent_comment_id\": \"$COMMENT_ID\"}")
    set REPLY_PARENT (echo $REPLY | python3 -c "import sys,json; print(json.load(sys.stdin)['parent_comment_id'])" 2>/dev/null)
    if test "$REPLY_PARENT" = "$COMMENT_ID"; pass; else; fail "Reply parent mismatch"; end
else
    fail "Reply (no post/comment ID)"
end

echo "Running test: Edit own comment"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set EDIT_C (curl -s -X PATCH $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID \
        -H "Content-Type: application/json" \
        -b $ANNA_JAR \
        -d '{"body": "Bearbeitet!"}')
    assert_json_value "Edit comment body" "$EDIT_C" "body" "Bearbeitet!"

    echo "Running test: Edit comment sets edited_at"
    set C_EDIT_AT (echo $EDIT_C | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if d.get('edited_at') else 'no')" 2>/dev/null)
    if test "$C_EDIT_AT" = "yes"; pass; else; fail "Comment edited_at"; end
else
    fail "Edit comment (no IDs)"; fail "Comment edited_at (no IDs)"
end

echo "Running test: Cannot edit other's comment"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set EDIT_OTHER (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID \
        -H "Content-Type: application/json" \
        -b $JAN_JAR \
        -d '{"body": "hacked"}')
    assert_status "Cannot edit other's comment" "400" "$EDIT_OTHER"
else
    fail "Cannot edit other's (no IDs)"
end

echo "Running test: List comments"
if test -n "$PHOTO_POST_ID"
    set C_LIST (curl -s $BASE/posts/$PHOTO_POST_ID/comments)
    set C_COUNT (echo $C_LIST | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
    if test "$C_COUNT" = "2"; pass; else; fail "Comment count (expected 2, got $C_COUNT)"; end
else
    fail "List comments (no post ID)"
end

echo "Running test: Post's denormalized comment_count reflects reality"
if test -n "$PHOTO_POST_ID"
    set PHOTO_POST_CHECK (curl -s $BASE/posts/$PHOTO_POST_ID)
    assert_json_value "Denormalized comment_count" "$PHOTO_POST_CHECK" "comment_count" "2"
else
    fail "Denormalized comment_count (no post ID)"
end

echo "Running test: Empty comment rejected"
if test -n "$PHOTO_POST_ID"
    set EMPTY_C (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/$PHOTO_POST_ID/comments \
        -H "Content-Type: application/json" \
        -b $ANNA_JAR \
        -d '{"body": ""}')
    assert_status "Empty comment" "400" "$EMPTY_C"
else
    fail "Empty comment (no post ID)"
end

echo "Running test: Comment without auth"
if test -n "$PHOTO_POST_ID"
    set C_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/$PHOTO_POST_ID/comments \
        -H "Content-Type: application/json" \
        -d '{"body": "nope"}')
    assert_status "Comment without auth" "401" "$C_NOAUTH"
else
    fail "Comment without auth (no post ID)"
end

# ══════════════════════════════════════════
# COMMENT LIKES
# ══════════════════════════════════════════

echo "Running test: Like a comment"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set CLIKE (curl -s -X POST $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID/like -b $JAN_JAR)
    assert_json_value "Comment like" "$CLIKE" "liked" "True"
    assert_json_value "Comment like count 1" "$CLIKE" "like_count" "1"
else
    fail "Comment like (no IDs)"; fail "Comment like count (no IDs)"
end

echo "Running test: Comment list reflects like"
if test -n "$PHOTO_POST_ID"
    set C_LIST_LIKED (curl -s "$BASE/posts/$PHOTO_POST_ID/comments" -b $JAN_JAR)
    set C_LIKE_COUNT (echo $C_LIST_LIKED | python3 -c "import sys,json; d=json.load(sys.stdin); c=[x for x in d if x['id']=='$COMMENT_ID']; print(c[0]['like_count'] if c else -1)" 2>/dev/null)
    if test "$C_LIKE_COUNT" = "1"; pass; else; fail "Comment list like_count (got $C_LIKE_COUNT)"; end
else
    fail "Comment list reflects like (no post ID)"
end

echo "Running test: Unlike a comment (toggle)"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set CUNLIKE (curl -s -X POST $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID/like -b $JAN_JAR)
    assert_json_value "Comment unlike" "$CUNLIKE" "liked" "False"
    assert_json_value "Comment unlike count 0" "$CUNLIKE" "like_count" "0"
else
    fail "Comment unlike (no IDs)"; fail "Comment unlike count (no IDs)"
end

echo "Running test: Like comment without auth"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set CLIKE_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID/like)
    assert_status "Like comment without auth" "401" "$CLIKE_NOAUTH"
else
    fail "Like comment without auth (no IDs)"
end

echo "Running test: Like nonexistent comment"
if test -n "$PHOTO_POST_ID"
    set CLIKE_GHOST (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/$PHOTO_POST_ID/comments/00000000-0000-0000-0000-000000000000/like -b $JAN_JAR)
    assert_status "Like nonexistent comment" "404" "$CLIKE_GHOST"
else
    fail "Like nonexistent comment (no post ID)"
end

# ══════════════════════════════════════════
# COMMENT DELETION
# ══════════════════════════════════════════

echo "Running test: Post owner can delete another user's comment"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    # jan owns the photo post; anna (COMMENT_ID's author) is not jan, but jan may still moderate
    set DEL_C (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID -b $JAN_JAR)
    assert_status "Post owner deletes comment" "204" "$DEL_C"
else
    fail "Post owner deletes comment (no IDs)"
end

echo "Running test: Deleted comment no longer listed"
if test -n "$PHOTO_POST_ID"
    set C_LIST_AFTER (curl -s $BASE/posts/$PHOTO_POST_ID/comments)
    set C_STILL_THERE (echo $C_LIST_AFTER | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if any(x['id']=='$COMMENT_ID' for x in d) else 'no')" 2>/dev/null)
    if test "$C_STILL_THERE" = "no"; pass; else; fail "Deleted comment still listed"; end
else
    fail "Deleted comment no longer listed (no post ID)"
end

echo "Running test: Delete already-deleted comment"
if test -n "$PHOTO_POST_ID"; and test -n "$COMMENT_ID"
    set DEL_C_AGAIN (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/posts/$PHOTO_POST_ID/comments/$COMMENT_ID -b $JAN_JAR)
    assert_status "Delete already-deleted comment" "404" "$DEL_C_AGAIN"
else
    fail "Delete already-deleted comment (no IDs)"
end

echo "Running test: Delete comment without auth"
set DEL_C_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/posts/00000000-0000-0000-0000-000000000000/comments/00000000-0000-0000-0000-000000000000)
assert_status "Delete comment without auth" "401" "$DEL_C_NOAUTH"

# ══════════════════════════════════════════
# DELETION & PERMISSIONS
# ══════════════════════════════════════════

echo "Running test: Anna cannot delete jan's post"
if test -n "$PHOTO_POST_ID"
    set DEL_OTHER (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/posts/$PHOTO_POST_ID -b $ANNA_JAR)
    assert_status "Cannot delete other's post" "400" "$DEL_OTHER"
else
    fail "Cannot delete other's post (no ID)"
end

echo "Running test: Anna cannot edit jan's post"
if test -n "$PHOTO_POST_ID"
    set EDIT_OTHER_P (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/posts/$PHOTO_POST_ID \
        -H "Content-Type: application/json" \
        -b $ANNA_JAR \
        -d '{"caption": "hacked"}')
    assert_status "Cannot edit other's post" "400" "$EDIT_OTHER_P"
else
    fail "Cannot edit other's post (no ID)"
end

echo "Running test: Jan deletes photo post"
if test -n "$PHOTO_POST_ID"
    set DEL_OWN (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/posts/$PHOTO_POST_ID -b $JAN_JAR)
    assert_status "Delete own post" "204" "$DEL_OWN"
else
    fail "Delete own post (no ID)"
end

echo "Running test: Deleted post returns 404"
if test -n "$PHOTO_POST_ID"
    set DEL_CHECK (curl -s -o /dev/null -w "%{http_code}" $BASE/posts/$PHOTO_POST_ID)
    assert_status "Deleted post 404" "404" "$DEL_CHECK"
else
    fail "Deleted post 404 (no ID)"
end

echo "Running test: Media files deleted from disk"
if test -n "$THUMB_FILE"; and test "$THUMB_FILE" != "uploads/"
    assert_file_missing "Thumb deleted" "$THUMB_FILE"
    assert_file_missing "Medium deleted" "$MEDIUM_FILE"
    assert_file_missing "Full deleted" "$FULL_FILE"
else
    pass; pass; pass
end

echo "Running test: Delete post without auth"
if test -n "$POST_ID"
    set DEL_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/posts/$POST_ID)
    assert_status "Delete without auth" "401" "$DEL_NOAUTH"
else
    fail "Delete without auth (no post ID)"
end

# ══════════════════════════════════════════
# PROFILE STATS & UNFOLLOW
# ══════════════════════════════════════════

echo "Running test: Profile stats"
set STATS (curl -s $BASE/users/$JAN_USER/stats)
assert_json_value "Stats followers" "$STATS" "followers" "1"
assert_json_value "Stats posts" "$STATS" "posts" "1"

echo "Running test: Unfollow"
set UNFOLLOW (curl -s -X DELETE $BASE/users/$JAN_USER/follow -b $ANNA_JAR)
assert_json_field "Unfollow" "$UNFOLLOW" "message"

echo "Running test: Stats after unfollow"
set STATS_AFTER (curl -s $BASE/users/$JAN_USER/stats)
assert_json_value "Followers after unfollow" "$STATS_AFTER" "followers" "0"

echo "Running test: Stats nonexistent user"
set STATS_NF (curl -s -o /dev/null -w "%{http_code}" $BASE/users/ghost_xyz/stats)
assert_status "Stats nonexistent" "404" "$STATS_NF"

# ══════════════════════════════════════════
# SEARCH
# ══════════════════════════════════════════

echo "Running test: Search finds jan"
set SEARCH_RES (curl -s "$BASE/users/search?q=$JAN_USER")
set SEARCH_FOUND (echo $SEARCH_RES | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if any(u['username']=='$JAN_USER' for u in d) else 'no')" 2>/dev/null)
if test "$SEARCH_FOUND" = "yes"; pass; else; fail "Search finds jan"; end

echo "Running test: Empty search query rejected"
set SEARCH_EMPTY_STATUS (curl -s -o /dev/null -w "%{http_code}" "$BASE/users/search?q=")
assert_status "Empty search query" "400" "$SEARCH_EMPTY_STATUS"

echo "Running test: Search with no matches returns empty array"
set SEARCH_NONE (curl -s "$BASE/users/search?q=zzz_definitely_nobody_zzz")
set SEARCH_NONE_COUNT (echo $SEARCH_NONE | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$SEARCH_NONE_COUNT" = "0"; pass; else; fail "Search no matches (got $SEARCH_NONE_COUNT)"; end

# ══════════════════════════════════════════
# BLOCKS
# ══════════════════════════════════════════

set BEN_USER "ben_$SUFFIX"
set BEN_EMAIL "$BEN_USER@example.com"
set BEN_JAR (mktemp)

echo "Running test: Register ben"
set BEN_RESPONSE (curl -s -c $BEN_JAR -X POST $BASE/auth/register \
    -H "Content-Type: application/json" \
    -d "{\"username\": \"$BEN_USER\", \"email\": \"$BEN_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Register ben username" "$BEN_RESPONSE" "user.username" "$BEN_USER"

echo "Running test: Jan blocks ben"
set BLOCK (curl -s -X POST $BASE/users/$BEN_USER/block -b $JAN_JAR)
assert_json_field "Block message" "$BLOCK" "message"

echo "Running test: Self-block rejected"
set SELF_BLOCK (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$JAN_USER/block -b $JAN_JAR)
assert_status "Self-block" "400" "$SELF_BLOCK"

echo "Running test: Block nonexistent user"
set BLOCK_GHOST (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/ghost_xyz/block -b $JAN_JAR)
assert_status "Block nonexistent" "404" "$BLOCK_GHOST"

echo "Running test: Duplicate block (idempotent)"
set DUP_BLOCK (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$BEN_USER/block -b $JAN_JAR)
assert_status "Duplicate block" "201" "$DUP_BLOCK"

echo "Running test: Blocked users list contains ben"
set BLOCKED_LIST (curl -s $BASE/users/me/blocked -b $JAN_JAR)
set BLOCKED_COUNT (echo $BLOCKED_LIST | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$BLOCKED_COUNT" = "1"; pass; else; fail "Blocked list count (got $BLOCKED_COUNT)"; end

echo "Running test: Blocked user cannot follow blocker"
set BEN_FOLLOW_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$JAN_USER/follow -b $BEN_JAR)
assert_status "Blocked cannot follow" "400" "$BEN_FOLLOW_STATUS"

echo "Running test: Blocked user cannot comment on blocker's post"
if test -n "$POST_ID"
    set BEN_COMMENT_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/posts/$POST_ID/comments \
        -H "Content-Type: application/json" \
        -b $BEN_JAR \
        -d '{"body": "sollte nicht gehen"}')
    assert_status "Blocked cannot comment" "400" "$BEN_COMMENT_STATUS"
else
    fail "Blocked cannot comment (no post ID)"
end

echo "Running test: Unblock ben"
set UNBLOCK (curl -s -X DELETE $BASE/users/$BEN_USER/block -b $JAN_JAR)
assert_json_field "Unblock message" "$UNBLOCK" "message"

echo "Running test: Blocked list empty after unblock"
set BLOCKED_AFTER (curl -s $BASE/users/me/blocked -b $JAN_JAR)
set BLOCKED_AFTER_COUNT (echo $BLOCKED_AFTER | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null)
if test "$BLOCKED_AFTER_COUNT" = "0"; pass; else; fail "Blocked list after unblock (got $BLOCKED_AFTER_COUNT)"; end

echo "Running test: Ben can follow jan after unblock"
set BEN_FOLLOW_AFTER (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$JAN_USER/follow -b $BEN_JAR)
assert_status "Follow after unblock" "201" "$BEN_FOLLOW_AFTER"

echo "Running test: Block without auth"
set BLOCK_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/users/$BEN_USER/block)
assert_status "Block without auth" "401" "$BLOCK_NOAUTH"

# ══════════════════════════════════════════
# CHATS
# ══════════════════════════════════════════

set CARLA_USER "carla_$SUFFIX"
set DAVE_USER "dave_$SUFFIX"
set ERIN_USER "erin_$SUFFIX"
set CARLA_JAR (mktemp)
set DAVE_JAR (mktemp)
set ERIN_JAR (mktemp)

echo "Running test: Register carla, dave, erin"
set CARLA_RESPONSE (curl -s -c $CARLA_JAR -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \"$CARLA_USER\", \"email\": \"$CARLA_USER@example.com\", \"password\": \"$PASSWORD\"}")
set DAVE_RESPONSE (curl -s -c $DAVE_JAR -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \"$DAVE_USER\", \"email\": \"$DAVE_USER@example.com\", \"password\": \"$PASSWORD\"}")
set ERIN_RESPONSE (curl -s -c $ERIN_JAR -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \"$ERIN_USER\", \"email\": \"$ERIN_USER@example.com\", \"password\": \"$PASSWORD\"}")
set CARLA_OK (echo $CARLA_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['user']['username'])" 2>/dev/null)
set DAVE_OK (echo $DAVE_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['user']['username'])" 2>/dev/null)
set ERIN_OK (echo $ERIN_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['user']['username'])" 2>/dev/null)
if test "$CARLA_OK" = "$CARLA_USER"; and test "$DAVE_OK" = "$DAVE_USER"; and test "$ERIN_OK" = "$ERIN_USER"
    pass
else
    fail "Register carla/dave/erin"
end

curl -s -X POST $BASE/users/$DAVE_USER/follow -b $CARLA_JAR > /dev/null
curl -s -X POST $BASE/users/$CARLA_USER/follow -b $DAVE_JAR > /dev/null
curl -s -X POST $BASE/users/$DAVE_USER/follow -b $ERIN_JAR > /dev/null

set DAVE_ID (echo $DAVE_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['user']['id'])" 2>/dev/null)
set CARLA_ID (echo $CARLA_RESPONSE | python3 -c "import sys,json; print(json.load(sys.stdin)['user']['id'])" 2>/dev/null)

echo "Running test: Send message between mutual followers"
set SEND_MSG (curl -s -X POST $BASE/chats/send \
    -H "Content-Type: application/json" \
    -b $CARLA_JAR \
    -d "{\"receiver_id\": \"$DAVE_ID\", \"body\": \"Hallo Dave!\"}")
set MESSAGE_ID (echo $SEND_MSG | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
assert_json_field "Send message" "$SEND_MSG" "id"
assert_json_value "Send message body" "$SEND_MSG" "body" "Hallo Dave!"

echo "Running test: Cannot message yourself"
set SELF_MSG_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/chats/send \
    -H "Content-Type: application/json" \
    -b $CARLA_JAR \
    -d "{\"receiver_id\": \"$CARLA_ID\", \"body\": \"hi me\"}")
assert_status "Cannot message self" "400" "$SELF_MSG_STATUS"

echo "Running test: Cannot message without mutual follow"
set NONMUTUAL_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/chats/send \
    -H "Content-Type: application/json" \
    -b $ERIN_JAR \
    -d "{\"receiver_id\": \"$DAVE_ID\", \"body\": \"hi dave\"}")
assert_status "Non-mutual message rejected" "403" "$NONMUTUAL_STATUS"

echo "Running test: Message without auth"
set MSG_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/chats/send \
    -H "Content-Type: application/json" \
    -d "{\"receiver_id\": \"$DAVE_ID\", \"body\": \"hi\"}")
assert_status "Message without auth" "401" "$MSG_NOAUTH"

echo "Running test: Conversation list for carla contains dave"
set CONVS (curl -s $BASE/chats -b $CARLA_JAR)
set CONV_ID (echo $CONVS | python3 -c "import sys,json; d=json.load(sys.stdin); m=[c for c in d if c['other_username']=='$DAVE_USER']; print(m[0]['id'] if m else '')" 2>/dev/null)
if test -n "$CONV_ID"; pass; else; fail "Conversation list contains dave"; end

echo "Running test: Get messages in conversation"
if test -n "$CONV_ID"
    set MSGS (curl -s $BASE/chats/$CONV_ID/messages -b $CARLA_JAR)
    set MSGS_HAS_IT (echo $MSGS | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if any(m['body']=='Hallo Dave!' for m in d) else 'no')" 2>/dev/null)
    if test "$MSGS_HAS_IT" = "yes"; pass; else; fail "Get messages contains sent message"; end
else
    fail "Get messages (no conversation ID)"
end

echo "Running test: Get messages without auth"
if test -n "$CONV_ID"
    set MSGS_NOAUTH (curl -s -o /dev/null -w "%{http_code}" $BASE/chats/$CONV_ID/messages)
    assert_status "Get messages without auth" "401" "$MSGS_NOAUTH"
else
    fail "Get messages without auth (no conversation ID)"
end

echo "Running test: Get messages for conversation you're not part of"
if test -n "$CONV_ID"
    set MSGS_FORBIDDEN (curl -s -o /dev/null -w "%{http_code}" $BASE/chats/$CONV_ID/messages -b $ERIN_JAR)
    assert_status "Get messages forbidden" "403" "$MSGS_FORBIDDEN"
else
    fail "Get messages forbidden (no conversation ID)"
end

echo "Running test: Edit own message"
if test -n "$MESSAGE_ID"
    set EDIT_MSG_STATUS (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/chats/messages/$MESSAGE_ID \
        -H "Content-Type: application/json" \
        -b $CARLA_JAR \
        -d '{"body": "Hallo Dave (bearbeitet)!"}')
    assert_status "Edit own message" "200" "$EDIT_MSG_STATUS"
else
    fail "Edit own message (no message ID)"
end

echo "Running test: Cannot edit someone else's message"
if test -n "$MESSAGE_ID"
    set EDIT_OTHER_MSG (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/chats/messages/$MESSAGE_ID \
        -H "Content-Type: application/json" \
        -b $DAVE_JAR \
        -d '{"body": "hacked"}')
    assert_status "Cannot edit other's message" "403" "$EDIT_OTHER_MSG"
else
    fail "Cannot edit other's message (no message ID)"
end

echo "Running test: Edit nonexistent message"
set EDIT_GHOST_MSG (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/chats/messages/00000000-0000-0000-0000-000000000000 \
    -H "Content-Type: application/json" \
    -b $CARLA_JAR \
    -d '{"body": "nope"}')
assert_status "Edit nonexistent message" "404" "$EDIT_GHOST_MSG"

echo "Running test: Toggle reaction on message"
if test -n "$MESSAGE_ID"
    set REACT_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/chats/messages/$MESSAGE_ID/reactions \
        -H "Content-Type: application/json" \
        -b $DAVE_JAR \
        -d '{"emoji": "\u2764\ufe0f"}')
    assert_status "Add reaction" "200" "$REACT_STATUS"

    set UNREACT_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/chats/messages/$MESSAGE_ID/reactions \
        -H "Content-Type: application/json" \
        -b $DAVE_JAR \
        -d '{"emoji": "\u2764\ufe0f"}')
    assert_status "Remove reaction (toggle)" "200" "$UNREACT_STATUS"
else
    fail "Add reaction (no message ID)"; fail "Remove reaction (no message ID)"
end

echo "Running test: Cannot delete someone else's message"
if test -n "$MESSAGE_ID"
    set DEL_OTHER_MSG (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/chats/messages/$MESSAGE_ID -b $DAVE_JAR)
    assert_status "Cannot delete other's message" "403" "$DEL_OTHER_MSG"
else
    fail "Cannot delete other's message (no message ID)"
end

echo "Running test: Delete own message"
if test -n "$MESSAGE_ID"
    set DEL_MSG_STATUS (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/chats/messages/$MESSAGE_ID -b $CARLA_JAR)
    assert_status "Delete own message" "204" "$DEL_MSG_STATUS"
else
    fail "Delete own message (no message ID)"
end

echo "Running test: Delete already-deleted message"
if test -n "$MESSAGE_ID"
    set DEL_MSG_AGAIN (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/chats/messages/$MESSAGE_ID -b $CARLA_JAR)
    assert_status "Delete already-deleted message" "404" "$DEL_MSG_AGAIN"
else
    fail "Delete already-deleted message (no message ID)"
end

# ══════════════════════════════════════════
# CHANGE PASSWORD
# ══════════════════════════════════════════

echo "Running test: Change password with wrong current password"
set WRONG_CURRENT (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me/password \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d "{\"current_password\": \"totallywrong\", \"new_password\": \"anothernewpassword1\"}")
assert_status "Wrong current password" "400" "$WRONG_CURRENT"

echo "Running test: Change password too short"
set SHORT_NEW (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me/password \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d "{\"current_password\": \"$PASSWORD\", \"new_password\": \"short\"}")
assert_status "New password too short" "400" "$SHORT_NEW"

echo "Running test: New password same as current rejected"
set SAME_PW (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me/password \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d "{\"current_password\": \"$PASSWORD\", \"new_password\": \"$PASSWORD\"}")
assert_status "Same password rejected" "400" "$SAME_PW"

echo "Running test: Change password successfully"
set FINAL_PW "nochNeueresPasswort789"
set CHANGE_PW_STATUS (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me/password \
    -H "Content-Type: application/json" \
    -b $JAN_JAR \
    -d "{\"current_password\": \"$PASSWORD\", \"new_password\": \"$FINAL_PW\"}")
assert_status "Change password" "204" "$CHANGE_PW_STATUS"

echo "Running test: Login with new password after change"
set RELOGIN (curl -s -c $JAN_JAR -b $JAN_JAR -X POST $BASE/auth/login -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"$FINAL_PW\"}")
assert_json_nested "Login after password change" "$RELOGIN" "user.username" "$JAN_USER"

echo "Running test: Login with old password fails after change"
set OLD_PW_AFTER_CHANGE (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$JAN_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_status "Old password fails after change" "400" "$OLD_PW_AFTER_CHANGE"

set PASSWORD "$FINAL_PW"

echo "Running test: Change password without auth"
set CHANGE_PW_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me/password \
    -H "Content-Type: application/json" \
    -d '{"current_password": "x", "new_password": "whatever123"}')
assert_status "Change password without auth" "401" "$CHANGE_PW_NOAUTH"

# ══════════════════════════════════════════
# DELETE ACCOUNT
# ══════════════════════════════════════════
# Uses a disposable throwaway user — destructive, so it must not touch
# jan/anna or any of the other users used elsewhere in this suite.

set EVE_USER "eve_$SUFFIX"
set EVE_EMAIL "$EVE_USER@example.com"
set EVE_JAR (mktemp)

echo "Running test: Register eve (throwaway)"
set EVE_RESPONSE (curl -s -c $EVE_JAR -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \"$EVE_USER\", \"email\": \"$EVE_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Register eve username" "$EVE_RESPONSE" "user.username" "$EVE_USER"

echo "Running test: Delete account without auth"
set DEL_ACCT_NOAUTH (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/users/me)
assert_status "Delete account without auth" "401" "$DEL_ACCT_NOAUTH"

echo "Running test: Delete own account"
set DEL_ACCT_STATUS (curl -s -o /dev/null -w "%{http_code}" -X DELETE $BASE/users/me -b $EVE_JAR)
assert_status "Delete own account" "204" "$DEL_ACCT_STATUS"

echo "Running test: Deleted user's public profile 404s"
set EVE_PROFILE_AFTER (curl -s -o /dev/null -w "%{http_code}" $BASE/users/$EVE_USER)
assert_status "Deleted user profile 404" "404" "$EVE_PROFILE_AFTER"

echo "Running test: Cannot log in as deleted user"
set EVE_LOGIN_AFTER (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/login \
    -H "Content-Type: application/json" \
    -d "{\"email\": \"$EVE_EMAIL\", \"password\": \"$PASSWORD\"}")
assert_status "Login as deleted user fails" "400" "$EVE_LOGIN_AFTER"

# ══════════════════════════════════════════
# USERNAME CASE HANDLING
# ══════════════════════════════════════════
# Usernames are stored exactly as entered, but uniqueness and every
# lookup are case-insensitive -- "CaseTest" and "casetest" are the same
# account and the same URL.

set CASE_USER "CaseTest_$SUFFIX"
set CASE_JAR (mktemp)

echo "Running test: Register with mixed-case username"
set CASE_RESPONSE (curl -s -c $CASE_JAR -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \"$CASE_USER\", \"email\": \"case_$SUFFIX@example.com\", \"password\": \"$PASSWORD\"}")
assert_json_nested "Register preserves entered case" "$CASE_RESPONSE" "user.username" "$CASE_USER"

echo "Running test: Duplicate registration rejected regardless of case"
set CASE_DUP_STATUS (curl -s -o /dev/null -w "%{http_code}" -X POST $BASE/auth/register -H "Content-Type: application/json" \
    -d "{\"username\": \""(string lower $CASE_USER)"\", \"email\": \"case2_$SUFFIX@example.com\", \"password\": \"$PASSWORD\"}")
assert_status "Duplicate registration case-insensitive" "409" "$CASE_DUP_STATUS"

echo "Running test: Lookup by lowercase URL finds the account"
set CASE_LOOKUP_LOWER (curl -s $BASE/users/(string lower $CASE_USER))
assert_json_value "Lookup lowercase preserves stored case" "$CASE_LOOKUP_LOWER" "username" "$CASE_USER"

echo "Running test: Lookup by uppercase URL finds the account"
set CASE_LOOKUP_UPPER (curl -s $BASE/users/(string upper $CASE_USER))
assert_json_value "Lookup uppercase preserves stored case" "$CASE_LOOKUP_UPPER" "username" "$CASE_USER"

echo "Running test: Stats lookup is also case-insensitive"
set CASE_STATS_STATUS (curl -s -o /dev/null -w "%{http_code}" $BASE/users/(string lower $CASE_USER)/stats)
assert_status "Stats lookup case-insensitive" "200" "$CASE_STATS_STATUS"

echo "Running test: Re-casing your own username is allowed (not blocked as self-conflict)"
set CASE_RECASE_STATUS (curl -s -o /dev/null -w "%{http_code}" -X PATCH $BASE/users/me \
    -H "Content-Type: application/json" \
    -b $CASE_JAR \
    -d "{\"username\": \""(string upper $CASE_USER)"\"}")
assert_status "Re-case own username" "200" "$CASE_RECASE_STATUS"

rm -f $CASE_JAR 2>/dev/null

# ══════════════════════════════════════════
# MISC
# ══════════════════════════════════════════

echo "Running test: Root index route"
set INDEX_STATUS (curl -s -o /dev/null -w "%{http_code}" $BASE/)
assert_status "Root index" "200" "$INDEX_STATUS"

echo "Running test: Health check"
set HEALTH (curl -s $BASE/health)
assert_json_value "Health status" "$HEALTH" "status" "ok"
assert_json_value "Health database" "$HEALTH" "database" "connected"

echo "Running test: User not found"
set UNF (curl -s -o /dev/null -w "%{http_code}" $BASE/users/nonexistent_xyz)
assert_status "User not found" "404" "$UNF"

echo "Running test: Get public user profile"
set PUB_USER (curl -s $BASE/users/$JAN_USER)
assert_json_value "Public profile username" "$PUB_USER" "username" "$JAN_USER"

# ══════════════════════════════════════════
# CLEANUP
# ══════════════════════════════════════════

rm -f $JAN_JAR $ANNA_JAR $BEN_JAR $CARLA_JAR $DAVE_JAR $ERIN_JAR $EVE_JAR $JAN_JAR_STALE $LOGOUT_JAR 2>/dev/null

# ══════════════════════════════════════════
# SUMMARY
# ══════════════════════════════════════════

set TOTAL (math $PASSED + $FAILED)
echo ""
echo "╔══════════════════════════════════════════╗"
echo "║              TEST RESULTS                ║"
echo "╠══════════════════════════════════════════╣"
printf "║  Total:  %-32s║\n" $TOTAL
printf "║  Passed: %-3s ✓%28s║\n" $PASSED ""
printf "║  Failed: %-3s ✗%28s║\n" $FAILED ""
echo "╚══════════════════════════════════════════╝"

if test $FAILED -gt 0
    echo ""
    echo "Failed tests:"
    for t in $FAILED_TESTS
        echo "  ✗ $t"
    end
    exit 1
else
    echo ""
    echo "All tests passed!"
    exit 0
end
