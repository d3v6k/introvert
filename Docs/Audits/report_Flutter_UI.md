# Group Chat UI – Deep Technical Audit Report

Below is the structured audit. Each issue references exact file paths and line numbers.

---

## QUESTION-BY-QUESTION ANSWERS

### Q1 — After acceptGroupInvite, does the contacts/group list refresh? Is a delay needed?

**File:** `lib/src/ui/main_shell.dart`, lines 1461–1464

```dart
onPressed: () {
  _client.acceptGroupInvite(groupId);
  Navigator.pop(context);
  _loadContacts();   // ← called immediately, zero delay
},
```

**Problem:** `_client.acceptGroupInvite()` is a fire-and-forget FFI call that enqueues a Rust DB write. `_loadContacts()` is called synchronously right after, before the Rust SQLite transaction has had a chance to commit. This means the newly joined group will often NOT appear in the list on the first refresh — the user must wait for the next network event to trigger `_debouncedLoadContacts()`.

**Status:** ⚠️ HIGH — race condition between Rust DB commit and Flutter query.

**Fix:**
```dart
onPressed: () async {
  _client.acceptGroupInvite(groupId);
  Navigator.pop(context);
  await Future.delayed(const Duration(milliseconds: 600)); // allow DB flush
  if (mounted) _loadContacts();
},
```

---

### Q2 — Does group_chat_screen listen to type==21 (new group message)? Does it refresh?

**File:** `lib/views/group_chat_screen.dart`, lines 1505–1506

```dart
if (event.type == 21) {
  _debouncedLoadMessages();
}
```

✅ **YES** — type==21 is handled. Messages are refreshed via `_debouncedLoadMessages()` (300 ms debounce → `_loadMessages()`). No issue here.

---

### Q3 — Does group_chat_screen listen to type==23 (group updated) and reload members/state?

**File:** `lib/views/group_chat_screen.dart`, lines 1507–1508

```dart
} else if (event.type == 23) {
  _debouncedReloadMessages();
}
```

**Problem:** When `type==23` fires (group updated — e.g., member added/removed, role changed), only `_debouncedReloadMessages()` is called. This calls `_reloadMessages()`, which calls `_doLoadContactNames()` indirectly via `_loadMessages()` → `_markMessagesAsRead()` … but **NOT directly**. More critically, `_debouncedReloadMessages` calls `_reloadMessages()`, which does NOT call `_loadContactNames()`. Member roster changes (new admins, new members) will NOT be reflected until the next `type==22` event (peer profile updated) happens to trigger `_loadContactNames()` + `_loadMessages()`.

Compare: `type==22` at line 1563–1565 correctly calls both:
```dart
} else if (event.type == 22) {
  _loadContactNames();
  _loadMessages();
}
```

**Status:** ⚠️ MEDIUM — member list can lag after a group membership/role change.

**Fix:**
```dart
} else if (event.type == 23) {
  _loadContactNames();   // add this
  _debouncedReloadMessages();
}
```

---

### Q4 — Is there stream subscription cleanup in dispose()? Check for _networkSubscription?.cancel()

**File:** `lib/views/group_chat_screen.dart`, lines 276–290

```dart
@override
void dispose() {
  _isDisposing = true;
  _loadMessagesDebounce?.cancel();
  _loadContactNamesDebounce?.cancel();
  _networkSubscription?.cancel();      // ✅ present
  _transferSubscription?.cancel();     // ✅ present
  _economySubscription?.cancel();      // ✅ present
  _recordingTimer?.cancel();
  _audioRecorder.dispose();
  _callExpiryTimer?.cancel();
  _pullRetryTimer?.cancel();
  _messageController.dispose();
  _scrollController.dispose();
  super.dispose();
}
```

✅ **All subscriptions and timers are cancelled in `dispose()`** — no leaks found here.

---

### Q5 — Does sendGroupMessage check that the group secret is non-zero before sending?

**File:** `lib/src/native/introvert_client.dart`, lines 1169–1177

```dart
void sendGroupMessage(String groupId, String message, [String? replyTo]) {
  using((Arena arena) {
    _groupSendMessage(
      groupId.toNativeUtf8(allocator: arena),
      message.toNativeUtf8(allocator: arena),
      (replyTo ?? "").toNativeUtf8(allocator: arena),
    );
  });
}
```

**Problem:** There is **zero validation** on the Flutter/Dart side before the FFI call. No check that `groupId` is non-empty, that the group exists in `getAllGroups()`, or that the group has a valid secret. If the group secret is not yet set (e.g., user just accepted an invite and the secret hasn't propagated from the Rust layer), the FFI call silently fails with no feedback to the user.

**Status:** ⚠️ HIGH — silent send failure if group not fully initialized.

**Fix:** Add a guard before calling `sendGroupMessage`, e.g., in `_sendMessage()` at line 1660:
```dart
final group = _client.getAllGroups().firstWhere(
  (g) => g is List && g.isNotEmpty && g[0] == widget.groupId,
  orElse: () => null,
);
if (group == null) {
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(content: Text("Group not ready. Please wait a moment.")),
  );
  return;
}
_client.sendGroupMessage(widget.groupId, text, replyToId);
```

---

### Q6 — After createGroup, does the UI navigate to the new group screen? Is there a delay?

**File:** `lib/src/ui/main_shell.dart`, lines 4685–4701

```dart
void _createGroup() {
  // ...
  _client.createGroup(name, desc, _selectedPeerIds);
  Navigator.pop(context);
  widget.onComplete();  // calls _loadContacts()
  messenger.showSnackBar(SnackBar(content: Text("Group '$name' created successfully!")));
}
```

**Problem 1:** After `createGroup()`, the code calls `widget.onComplete()` which resolves to `_loadContacts()`. Like Q1, this fires **immediately** with no delay, before the Rust DB write has committed. The group will often **not appear** in the list on the first refresh.

**Problem 2:** The UI does **NOT navigate** to the new group's `GroupChatScreen`. The user is just returned to the contacts/groups list and has to manually tap into the new group. This is a UX gap — there is no deep-link to the newly created group.

**Status:** ⚠️ HIGH — no navigation to new group; MEDIUM — delay issue.

**Fix:**
```dart
void _createGroup() async {
  _client.createGroup(name, desc, _selectedPeerIds);
  Navigator.pop(context);
  await Future.delayed(const Duration(milliseconds: 600));
  widget.onComplete(); // refresh list after DB write flushes
  // Optionally: navigate to the new group (requires returning the new groupId from FFI)
}
```

---

### Q7 — Are there async gaps where widget could be disposed between await and setState?

Several `async` methods lack a post-`await` `mounted` check:

**7a. `_pickAndSendImage()` — line 2020–2052**
```dart
void _pickAndSendImage() async {
  try {
    final pickedFiles = await ImagePicker().pickMultiImage(...);  // ← await
    if (pickedFiles.isNotEmpty) {
      for (var pickedFile in pickedFiles) {
        ...
        _client.sendGroupMessage(...);  // ← no `if (!mounted) return` guard
      }
      _loadMessages();   // ← calls setState internally, no mounted check at call site
    }
  } catch (_) {}
}
```

**Status:** ⚠️ HIGH — `_loadMessages()` calls `setState()` inside, which will throw if unmounted. No `if (!mounted) return` after the `await`.

**7b. `_pickAndSendVideo()` — line 2071–2093**
Same pattern. No `mounted` check after `await ImagePicker().pickVideo(...)`.

**Status:** ⚠️ HIGH

**7c. `_sendFile()` — line 2096–2119**
Same pattern. No `mounted` check after `await FilePicker.platform.pickFiles(...)`.

**Status:** ⚠️ HIGH

**7d. `_shareLocation()` — line 2122–2150**
```dart
final result = await Navigator.push(...);  // ← await
if (result != null) {
  _client.sendGroupMessage(widget.groupId, text);
  _loadMessages();   // ← no mounted guard
}
```

**Status:** ⚠️ HIGH — widget can be popped while map picker is open.

**7e. `_stopRecordingAndSend()` — line 2502–2553**
```dart
final appDir = await getApplicationDocumentsDirectory();  // ← await
...
if (mounted) { setState(() => _isRecording = false); }   // ✅ guarded
...
_client.sendGroupMessage(...);   // no mounted guard
_loadMessages();                 // no mounted guard after multiple awaits
```

**Status:** ⚠️ MEDIUM — `_isRecording` is guarded but `_loadMessages()` at line 2544 is not.

**7f. Delete message dialog in `_buildSelectionToolbar()` — line 1160–1178**
```dart
onPressed: () async {
  final confirm = await showDialog<bool>(...);  // ← await
  if (confirm == true) {
    _client.deleteMessage(...);
    setState(() => _selectedMsg = null);  // ← no mounted guard
    _loadMessages();
  }
},
```

**Status:** ⚠️ HIGH — `setState()` without a `mounted` check after await.

**7g. Delete dialog in `_showMessageActions()` bottom sheet — line 1317–1346**
```dart
onTap: () async {
  Navigator.pop(context);
  final confirm = await showDialog<bool>(...);  // ← await
  if (confirm == true && msgId != null) {
    _client.deleteMessage(...);
    setState(() { ... });  // ← no mounted guard
  }
},
```

**Status:** ⚠️ HIGH

**General Fix Pattern:**
```dart
if (!mounted) return;
setState(() { ... });
```

---

### Q8 — Does _showInvitePrompt guard against duplicates? What if two invites arrive within milliseconds?

**File:** `lib/src/ui/main_shell.dart`, lines 1419–1473

```dart
void _showInvitePrompt(String groupId, ...) {
  if (_activeGroupInviteIds.contains(groupId)) {
    debugPrint("already open, ignoring duplicate.");
    return;
  }
  _activeGroupInviteIds.add(groupId);   // ✅ added before showDialog
  showDialog(...).then((_) {
    _activeGroupInviteIds.remove(groupId);  // ✅ removed on close
  });
}
```

✅ **The deduplication is correct.** `_activeGroupInviteIds` is a `Set<String>` populated before `showDialog` is called. Concurrent invites for the same `groupId` within the same event loop tick will be blocked.

**Minor caveat:** The dedup is per `groupId`. If the same user sends two different invites at exactly the same time (via two separate network events), and both arrive in the same event loop iteration, both could slip through before `_activeGroupInviteIds.add()` is called. But since `_showInviteDialog` is wrapped in `Future.microtask()` (line 1383–1385), and microtasks drain sequentially, this is safe in practice.

**Status:** ✅ No issue found.

---

### Q9 — Is there proper handling when getGroupMessages returns empty?

**File:** `lib/views/group_chat_screen.dart`, lines 519–716 (`_reloadMessages`)

```dart
final msgs = _client.getGroupMessages(widget.groupId);
setState(() {
  final List<dynamic> processed = [];
  for (var m in msgs) {
    if (m == null || m.length < 5) continue;
    ...
  }
  _messages = processed;
  _messagesVersion++;
});
```

When `getGroupMessages()` returns `[]`, the loop body never executes and `_messages` is set to an empty list (`processed = []`). The widget's `build()` method uses `_displayMessages` which returns an empty list, meaning a blank chat body is rendered. There is no empty-state widget (e.g., "No messages yet — say hello!").

**Status:** ⚠️ LOW — functional but poor UX with no empty state message.

**Fix:** In the `build()` method, check if `_displayMessages.isEmpty` and show a placeholder widget.

---

### Q10 — Check for 'mounted' guards missing after async operations.

Already covered in Q7. Summary of all missing `mounted` guards:

| File | Line | Method | Issue |
|------|------|--------|-------|
| group_chat_screen.dart | 2020–2052 | `_pickAndSendImage()` | No `mounted` check after `await pickMultiImage` |
| group_chat_screen.dart | 2071–2093 | `_pickAndSendVideo()` | No `mounted` check after `await pickVideo` |
| group_chat_screen.dart | 2096–2119 | `_sendFile()` | No `mounted` check after `await pickFiles` |
| group_chat_screen.dart | 2139–2149 | `_shareLocation()` | No `mounted` check after `await Navigator.push` |
| group_chat_screen.dart | 2529–2544 | `_stopRecordingAndSend()` | No `mounted` check before `_loadMessages()` at end |
| group_chat_screen.dart | 1160–1178 | delete in `_buildSelectionToolbar` | No `mounted` check after `await showDialog` |
| group_chat_screen.dart | 1317–1346 | delete in `_showMessageActions` | No `mounted` check after `await showDialog` |

---

## SUMMARY TABLE

| # | Severity | File | Lines | Issue |
|---|----------|------|-------|-------|
| 1 | **HIGH** | main_shell.dart | 1461–1464 | `acceptGroupInvite` → `_loadContacts()` race with Rust DB write |
| 2 | **HIGH** | main_shell.dart | 4695–4697 | `createGroup` → `onComplete()` race + no navigation to new group |
| 3 | **HIGH** | introvert_client.dart | 1169–1177 | `sendGroupMessage` has no group-exists / secret-non-zero validation |
| 4 | **HIGH** | group_chat_screen.dart | 2020–2052 | `_pickAndSendImage` missing `mounted` guard after await |
| 5 | **HIGH** | group_chat_screen.dart | 2071–2093 | `_pickAndSendVideo` missing `mounted` guard after await |
| 6 | **HIGH** | group_chat_screen.dart | 2096–2119 | `_sendFile` missing `mounted` guard after await |
| 7 | **HIGH** | group_chat_screen.dart | 2139–2149 | `_shareLocation` missing `mounted` guard after await |
| 8 | **HIGH** | group_chat_screen.dart | 1160–1178 | Delete via toolbar: missing `mounted` after `await showDialog` |
| 9 | **HIGH** | group_chat_screen.dart | 1317–1346 | Delete via action sheet: missing `mounted` after `await showDialog` |
| 10 | **MEDIUM** | group_chat_screen.dart | 1507–1508 | `type==23` only calls `_debouncedReloadMessages`, skips `_loadContactNames()` |
| 11 | **MEDIUM** | group_chat_screen.dart | 2529–2544 | `_stopRecordingAndSend` missing `mounted` before `_loadMessages()` at end |
| 12 | **LOW** | group_chat_screen.dart | 519–716 | Empty message list has no empty-state UI placeholder |
| ✅ | PASS | group_chat_screen.dart | 276–290 | All stream subscriptions + timers correctly cancelled in `dispose()` |
| ✅ | PASS | group_chat_screen.dart | 1505–1506 | type==21 (new group message) correctly triggers message refresh |
| ✅ | PASS | main_shell.dart | 1419–1473 | `_showInvitePrompt` correctly deduplicates concurrent invites |
