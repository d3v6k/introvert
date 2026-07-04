import 'package:flutter/material.dart';
import '../src/ui/widgets/messenger_webview.dart';

class TelegramWebTab extends StatefulWidget {
  final void Function(int unreadCount)? onUnreadCountChanged;

  const TelegramWebTab({super.key, this.onUnreadCountChanged});

  @override
  State<TelegramWebTab> createState() => _TelegramWebTabState();
}

class _TelegramWebTabState extends State<TelegramWebTab>
    with AutomaticKeepAliveClientMixin {
  @override
  bool get wantKeepAlive => true;

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return MessengerWebView(
      url: 'https://web.telegram.org/k/',
      title: 'Telegram',
      icon: Icons.send_rounded,
      accentColor: const Color(0xFF0088CC),
      allowedDomain: 'telegram.org',
      onUnreadCountChanged: widget.onUnreadCountChanged,
    );
  }
}
