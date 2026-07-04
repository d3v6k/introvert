# UI Component Manifest

## 1. Interaction Design Philosophy
Introvert's UI is designed to feel "native" and responsive. It uses a **reactive event-driven model** where the UI state is a direct reflection of the underlying Rust network state.

## 2. Core Screens

### `MainShell` (`lib/src/ui/main_shell.dart`)
- **Purpose:** The application scaffold.
- **Functionality:** 
    - Manages the Bottom Navigation Bar (Chats, WhatsApp*, Telegram*, Drive, Notes, Settings). *WhatsApp/Telegram tabs are optional, enabled in Settings.
    - Contains the Global Status Bar (Syncing, Active, Relay).
    - Listens to the Rust `eventStream` and routes global events (like background message receipts) to the appropriate sub-components.
    - Intro-Claw engine expander in Settings (description, activity log, RECON/HEAL buttons).

### `ChatScreen` (`lib/views/chat_screen.dart`)
- **Purpose:** The primary interaction point for 1-on-1 chats.
- **Key Logic:**
    - Uses a `ListView.builder` for efficient message rendering.
    - Implements WhatsApp-style delivery receipts (Ticks) by mapping Event 13 status codes.
    - Preserves `localPath` in the `FileTransferProgress` stream to ensure media thumbnails don't disappear after completion.

### `GroupChatScreen` (`lib/views/group_chat_screen.dart`)
- **Purpose:** Decentralized multi-user rooms.
- **Key Logic:**
    - Listens for **Event Type 21** (Group Messages) and refreshes history.
    - Implements **Mesh Intelligence** dialog for cryptographic member management (Add/Remove/Promote).
    - Supports **Mesh Code** sharing for open decentralized joining.

### `ContactScreen` (`lib/views/contact_screen.dart`)
- **Purpose:** Identity and relationship management.
- **Features:** 
    - Wormhole Invite Dialog: Uses the Magic Wormhole Rust module to generate/input codes.
    - Contact Deletion: Triggers a cascade delete in the Rust SQLCipher DB to wipe all shared history.

### `OnboardingScreen` (`lib/src/ui/onboarding_screen.dart`)
- **Purpose:** New user acquisition.
- **Flow:** 
    - Splash -> Mnemonic Generation -> Mnemonic Verification -> Identity Activation.
    - Seed is passed once to the Rust core via `introvert_identity_create` and never stored in plain text on the Dart side.

### `WhatsAppWebTab` (`lib/views/whatsapp_web_tab.dart`)
- **Purpose:** Embedded WhatsApp Web client via WebView.
- **Key Logic:**
    - Loads `web.whatsapp.com` in `flutter_inappwebview` WebView.
    - Parses page title for unread count (e.g., "(3) WhatsApp") and shows badge on tab icon.
    - Cookie persistence keeps user logged in between sessions.
    - Introvert-themed glassmorphic bottom bar with back/forward/refresh/home.

### `TelegramWebTab` (`lib/views/telegram_web_tab.dart`)
- **Purpose:** Embedded Telegram Web client via WebView.
- **Key Logic:**
    - Loads `web.telegram.org/k/` in `flutter_inappwebview` WebView.
    - Parses page title for unread count and shows badge on tab icon.
    - Cookie persistence keeps user logged in between sessions.
    - Introvert-themed glassmorphic bottom bar with back/forward/refresh/home.

### `MessengerWebView` (`lib/src/ui/widgets/messenger_webview.dart`)
- **Purpose:** Reusable WebView wrapper with Introvert theming.
- **Key Logic:**
    - Glassmorphic AppBar matching Introvert's theme.
    - Loading progress bar with custom accent color.
    - Title change listener for unread badge count extraction.
    - Bottom navigation bar with back/forward/refresh/home buttons.

## 3. Specialized Widgets

### `FileTransferBubble` (`lib/src/ui/widgets/file_transfer_bubble.dart`)
- **Complexity:** High.
- **Capabilities:**
    - Dynamic progress ring for active transfers.
    - MIME-type detection for specialized rendering (Images, Videos, PDFs).
    - "Open File" integration using the `open_file` package.
    - Blur-preview for incoming media before the transfer completes.

### `SovereignAvatar` (`lib/src/ui/widgets/sovereign_avatar.dart`)
- **Logic:**
    - If a user has not set a base64 avatar, it renders a deterministic **JDenticon** based on the peer's unique libp2p `PeerId`.
    - This ensures every peer has a recognizable visual signature even in pure zero-knowledge mode.

### `_JoinMeshDialog` (Internal)
- **Purpose:** Open mesh joining via code.
- **Logic:** Decrypts Kademlia manifests using a user-provided passphrase and triggers group subscription.

### `RewardsHud` (`lib/src/ui/widgets/rewards_hud.dart`)
- **Logic:** 
    - Displays real-time relay statistics (bytes forwarded).
    - Interfaces with the `economy` module to show pending $INTR rewards.
