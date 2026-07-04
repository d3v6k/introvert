# Onboarding Guide

## 1. First Launch Flow

### Step 1: Welcome Screen
- App displays introvert logo and tagline
- "Get Started" button initiates onboarding

### Step 2: Identity Generation
- App generates 32-byte master seed
- BIP39 mnemonic displayed (24 words)
- **CRITICAL:** User must save these words securely
- Loss = permanent identity loss (no recovery)

### Step 3: Seed Backup
- User prompted to write down mnemonic
- Verification step: Enter 3 random words
- "I've saved my seed" confirmation

### Step 4: Profile Setup
- Display name (optional)
- Handle (optional, requires PoW)
- Avatar (optional)

### Step 5: Engine Start
- Rust core initialized with seed
- SQLCipher database created
- libp2p swarm started
- Connection to RBN established

## 2. Device Pairing (Magic Wormhole)

### Existing Device
1. Open Settings → Add Device
2. Tap "Generate Pairing Code"
3. 2-word code displayed (e.g., "7-correct-horse")
4. Wait for new device to connect

### New Device
1. Open Introvert for first time
2. Select "Pair with Existing Device"
3. Enter 2-word code from old device
4. Wait for identity exchange
5. Contacts and groups synced

### Technical Flow
1. Both devices connect to `relay.magic-wormhole.io`
2. Wormhole protocol establishes secure channel
3. `SovereignIdentity` packages exchanged
4. Identities stored in `contacts` table
5. Direct P2P connection established

## 3. Contact Addition

### Method 1: Wormhole Pairing
See Device Pairing above.

### Method 2: Handle Resolution
1. Open Chats → New Chat
2. Enter handle (e.g., "@alice")
3. `introvert_network_resolve_handle(handle)` called
4. DHT lookup returns PeerId
5. Direct connection established

### Method 3: QR Code (Future)
- Display QR code with PeerId
- Scan on other device
- Automatic connection

## 4. Group Creation

### Step 1: Create Group
1. Open Chats → New Group
2. Enter group name
3. Add members (from contacts)

### Step 2: Secret Generation
- 32-byte symmetric key generated
- Encrypted with each member's public key
- Distributed via Noise-encrypted signaling

### Step 3: Gossipsub Topic
- Group subscribes to Gossipsub topic
- Messages encrypted with group secret
- Signed actions for authorization

## 5. First Message

### Direct Message
1. Open contact chat
2. Type message
3. `introvert_network_send_message` called
4. Message encrypted and sent via signaling
5. Status ticks: Sent → Delivered → Read

### Group Message
1. Open group chat
2. Type message
3. `GroupAction::Message` signed and broadcast
4. Gossipsub propagates to all members
5. Decrypted locally with group secret

## 6. File Sharing

### Send File
1. Open chat
2. Tap attachment icon
3. Select file from device
4. File offer sent to recipient
5. Chunks transferred
6. SHA-256 verification
7. File saved to recipient's drive

### Receive File
1. File offer notification
2. Accept transfer
3. Chunks received and reassembled
4. Hash verified
5. File saved to Sovereign Drive
6. Thumbnail generated

## 7. First Call

### Voice Call
1. Open contact chat
2. Tap phone icon
3. WebRTC peer connection established
4. Audio stream via libp2p signaling
5. Call controls (mute, speaker, end)

### Video Call
1. Open contact chat
2. Tap video icon
3. Camera and microphone permissions
4. WebRTC video stream
5. Picture-in-picture support

## 8. Sovereign Drive

### Upload File
1. Open Drive tab
2. Tap upload button
3. Select file
4. File copied to persistent storage
5. Metadata registered

### Share from Drive
1. Open Drive tab
2. Long-press file
3. Select "Share to Chat"
4. Choose contact or group
5. File forwarded without re-upload

## 9. Handle Registration

### Claim Handle
1. Open Settings → Claim Handle
2. Enter desired handle
3. PoW computation (difficulty: 4)
4. Claim broadcast to mesh
5. RBN witnesses verify and sign
6. Handle confirmed and resolved

### Verification
- Other users can verify handle ownership
- Quorum of RBN signatures required
- Prevents impersonation and squatting

## 10. Settings & Customization

### Theme Selection
- 5 built-in themes
- Persistent across sessions
- Real-time preview

### Notification Settings
- Call notifications
- Message notifications
- Background service toggle

### Debug Options
- Connection diagnostics overlay
- Network status display
- Event log viewer
