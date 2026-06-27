# iOS Push Notification Configuration Guide

This guide walks through setting up Apple Push Notification service (APNs) for Introvert so the RBN can wake iOS devices when new messages arrive.

## Prerequisites

- Apple Developer Account ($99/year)
- A Mac with Xcode installed (for certificate management)
- Access to the RBN server

---

## Step 1: Enroll in Apple Developer Program

If you don't have an account yet:

1. Go to [https://developer.apple.com/programs/enroll/](https://developer.apple.com/programs/enroll/)
2. Sign in with your Apple ID
3. Enroll as an Individual or Organization ($99/year)
4. Wait for approval (usually instant for individuals, may take days for organizations)

---

## Step 2: Create an App ID

1. Go to [Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources/identifiers/list)
2. Click the **+** button to register a new identifier
3. Select **App IDs** → Continue
4. Select **App** → Continue
5. Fill in:
   - **Description:** Introvert
   - **Bundle ID:** Select "Explicit" and enter `chat.introvert.app`
6. Under **Capabilities**, check **Push Notifications**
7. Click **Continue** → **Register**

---

## Step 3: Create an APNs Auth Key (.p8)

This is the recommended approach — a single key works for both development and production.

1. Go to [Keys](https://developer.apple.com/account/resources/authkeys/list)
2. Click the **+** button to create a new key
3. Fill in:
   - **Key Name:** Introvert APNs
4. Check **Apple Push Notifications service (APNs)**
5. Click **Continue** → **Register**
6. **Download the `.p8` file** — you can only download it once!
7. Note the **Key ID** (10-character string shown on the key details page)

### Important
- Save the `.p8` file securely — it cannot be re-downloaded
- You only need one key for all your apps and environments

---

## Step 4: Get Your Team ID

1. Go to [Membership](https://developer.apple.com/account/)
2. Find your **Team ID** (10-character alphanumeric string under the team name)

---

## Step 5: Configure the RBN Server

### 5a. Place the APNs key file

Copy your `.p8` file into the project:

```bash
cp ~/Downloads/AuthKey_XXXXXXXXXX.p8 firebase/apns-key.p8
```

### 5b. Update the systemd service file

Edit `for_linux/introvertd.service` and fill in your values:

```ini
Environment="APNS_KEY_ID=YOUR_KEY_ID"
Environment="APNS_TEAM_ID=YOUR_TEAM_ID"
```

The other APNs variables have sensible defaults:
- `APNS_KEY_PATH=/opt/introvert/config/apns-key.p8` (set by deploy script)
- `APNS_BUNDLE_ID=chat.introvert.app` (matches your App ID)
- `APNS_USE_PRODUCTION=true` (use `false` for TestFlight/development)

### 5c. Deploy

```bash
./deploy_rbn.sh
```

The deploy script will:
1. Upload the compiled binary
2. Upload the Firebase service account
3. Upload `firebase/apns-key.p8` to `/opt/introvert/config/`
4. Upload the updated systemd service file
5. Restart the daemon

### 5d. Verify

SSH into the RBN and check the logs:

```bash
ssh root@47.89.252.80
journalctl -u introvertd -f | grep -i "push\|apns"
```

You should see:
```
[Push] ✅ APNs configuration loaded
```

If you see `[Push] ⚠️ No APNs config — iOS push disabled`, check that:
- The `.p8` file exists at `/opt/introvert/config/apns-key.p8`
- The `APNS_KEY_ID` and `APNS_TEAM_ID` environment variables are set
- The systemd service was reloaded (`systemctl daemon-reload`)

---

## Step 6: Xcode Configuration (iOS App)

### 6a. Enable Push Notifications capability

1. Open `ios/Runner.xcworkspace` in Xcode
2. Select the **Runner** target
3. Go to **Signing & Capabilities**
4. Click **+ Capability**
5. Add **Push Notifications**
6. Also ensure **Background Modes** is enabled with:
   - ✅ Background fetch
   - ✅ Remote notifications

### 6b. Verify Info.plist

The following should already be present in `ios/Runner/Info.plist`:

```xml
<key>UIBackgroundModes</key>
<array>
    <string>remote-notification</string>
    <string>fetch</string>
</array>
```

### 6c. Verify entitlements

Check `ios/Runner/Runner.entitlements` contains:

```xml
<key>aps-environment</key>
<string>development</string>
```

For App Store/TestFlight builds, change this to:
```xml
<key>aps-environment</key>
<string>production</string>
```

Or use Xcode's automatic signing to handle this.

---

## Step 7: Register for Push in the App

The app already handles this automatically:

1. `AppDelegate.swift` requests notification permissions on launch
2. On grant, it calls `registerForRemoteNotifications()`
3. The APNs device token is sent to Flutter via `onDeviceToken` MethodChannel
4. `AlertService` forwards the token to the Rust engine via `registerPushToken()`
5. The engine sends `IdentifySleepState` to the RBN, which stores the token

No additional code changes needed.

---

## Testing

### Development (sandbox)
- Build and run from Xcode on a physical device (simulator doesn't support push)
- Set `APNS_USE_PRODUCTION=false` in the service file for sandbox pushes
- The app must be backgrounded or closed to receive pushes

### Production
- Set `APNS_USE_PRODUCTION=true` (default)
- Works for TestFlight and App Store builds

### Verify token registration
On the RBN, check the stored token:

```bash
sqlite3 /opt/introvert/data/introvertd.sqlite3 \
  "SELECT peer_id, device_type, last_seen FROM push_tokens WHERE device_type='ios';"
```

### Send a test push
You can test the APNs connection directly with curl:

```bash
# Generate a JWT token from the P8 key (requires a script or tool)
# Then send a test push:
curl -v \
  --http2 \
  --header "authorization: bearer YOUR_JWT_TOKEN" \
  --header "apns-topic: chat.introvert.app" \
  --header "apns-push-type: alert" \
  --header "apns-priority: 10" \
  --data '{"aps":{"alert":{"title":"Test","body":"Hello from APNs"},"sound":"default"}}' \
  https://api.push.apple.com/3/device/DEVICE_TOKEN
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `[Push] ⚠️ No APNs config` | Missing env vars or `.p8` file | Check Step 5 |
| `BadDeviceToken` in logs | Token from wrong environment (dev vs prod) | Match `APNS_USE_PRODUCTION` to build type |
| Push sent but no notification | App not requesting permissions | Check Xcode Push Notifications capability |
| `InvalidProviderToken` | Wrong Key ID or Team ID | Double-check values in service file |
| `Forbidden` from APNs | Key doesn't have APNs enabled | Recreate key with APNs capability checked |
| Token never appears on RBN | App can't reach RBN | Check network connectivity, ensure app registers on launch |

---

## Environment Variables Reference

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `APNS_KEY_PATH` | Yes | — | Path to `.p8` key file on RBN |
| `APNS_KEY_ID` | Yes | — | Apple Push Key ID (10 chars) |
| `APNS_TEAM_ID` | Yes | — | Apple Developer Team ID (10 chars) |
| `APNS_BUNDLE_ID` | No | `chat.introvert.app` | iOS app bundle ID |
| `APNS_USE_PRODUCTION` | No | `true` | `true` = production, `false` = sandbox |

---

## Security Notes

- The `.p8` key file grants the ability to send push notifications to any device with your app — treat it like a private key
- Never commit the `.p8` file to git (it's in `firebase/` which should be gitignored)
- The key does not expire, but you can revoke it from the Apple Developer portal if compromised
- Consider rotating the key annually as a best practice
