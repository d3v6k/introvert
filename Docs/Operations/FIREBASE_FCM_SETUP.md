# Firebase FCM Setup Guide

## Step 1: Create Firebase Project

1. Go to [Firebase Console](https://console.firebase.google.com/)
2. Click "Add project"
3. Enter project name: `introvert-p2p`
4. Disable Google Analytics (not needed)
5. Click "Create project"

## Step 2: Add Android App

1. In Firebase Console, click "Add app" → Android
2. Enter package name: `com.example.introvert`
3. Download `google-services.json`
4. Replace the placeholder file at `android/app/google-services.json`

## Step 3: Enable Cloud Messaging

1. In Firebase Console → Project Settings → Cloud Messaging
2. Ensure Firebase Cloud Messaging API (V1) is enabled
3. Note the Server Key (for RBN push relay later)

## Step 4: Test

1. Run the app on a real Android device (not emulator)
2. Check logcat for: `FCM token received: ...`
3. The token is automatically registered with the RBN

## Step 5: RBN Push Relay (Phase 2)

Once FCM is working, the RBN needs to:
1. Store push tokens when peers register
2. Send FCM push when message is stored in mailbox
3. Use the Server Key from Step 3

## Troubleshooting

- **No FCM token**: Check that `google-services.json` is correct
- **Push not received**: Check Firebase Console → Cloud Messaging → send test message
- **Service not starting**: Check logcat for `IntrovertFirebaseMessagingService`
