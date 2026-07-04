import 'package:flutter/material.dart';
import '../src/ui/widgets/messenger_webview.dart';
import '../src/ui/widgets/whatsapp_icon.dart';

class WhatsAppWebTab extends StatefulWidget {
  final void Function(int unreadCount)? onUnreadCountChanged;

  const WhatsAppWebTab({super.key, this.onUnreadCountChanged});

  @override
  State<WhatsAppWebTab> createState() => _WhatsAppWebTabState();
}

class _WhatsAppWebTabState extends State<WhatsAppWebTab>
    with AutomaticKeepAliveClientMixin {
  @override
  bool get wantKeepAlive => true;

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return MessengerWebView(
      url: 'https://web.whatsapp.com',
      title: 'WhatsApp',
      icon: Icons.chat_rounded,
      customIcon: const WhatsAppIcon(size: 24, color: Color(0xFF25D366)),
      accentColor: const Color(0xFF25D366),
      allowedDomain: 'whatsapp.com',
      onUnreadCountChanged: widget.onUnreadCountChanged,
    );
  }
}
