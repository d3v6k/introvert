import 'dart:collection';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';
import 'package:url_launcher/url_launcher.dart';
import '../../../theme/app_theme.dart';

class MessengerWebView extends StatefulWidget {
  final String url;
  final String title;
  final IconData icon;
  final Widget? customIcon;
  final Color accentColor;
  final String? allowedDomain;
  final void Function(int unreadCount)? onUnreadCountChanged;

  const MessengerWebView({
    super.key,
    required this.url,
    required this.title,
    required this.icon,
    this.customIcon,
    required this.accentColor,
    this.allowedDomain,
    this.onUnreadCountChanged,
  });

  @override
  State<MessengerWebView> createState() => _MessengerWebViewState();
}

class _MessengerWebViewState extends State<MessengerWebView>
    with AutomaticKeepAliveClientMixin {
  InAppWebViewController? _webViewController;
  bool _isLoading = true;
  bool _showSetupGuide = true;
  bool _hasCheckedLoginState = false;
  int _unreadCount = 0;
  String? _currentUrl;
  double _loadProgress = 0;
  bool _showNavBar = false;

  // Desktop User-Agent to avoid mobile detection / bot blocking
  static const String _desktopUserAgent =
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36';

  @override
  bool get wantKeepAlive => true;

  @override
  void initState() {
    super.initState();
    // Timeout: if login state isn't detected within 5 seconds, show setup guide
    Future.delayed(const Duration(seconds: 5), () {
      if (mounted && !_hasCheckedLoginState) {
        setState(() => _hasCheckedLoginState = true);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    super.build(context);
    final statusBar = MediaQuery.of(context).padding.top;
    final headerHeight = statusBar + kToolbarHeight;
    final topOffset = headerHeight / 2;
    final navBarHeight = MediaQuery.of(context).padding.bottom + 68;
    final bottomOffset = (navBarHeight - topOffset).clamp(0.0, navBarHeight);
    return Padding(
      padding: EdgeInsets.only(top: topOffset, bottom: bottomOffset),
      child: Stack(
        children: [
          InAppWebView(
                initialUrlRequest: URLRequest(url: WebUri(widget.url)),
                initialSettings: InAppWebViewSettings(
                  javaScriptEnabled: true,
                  // Persistent storage — survives app restarts
                  domStorageEnabled: true,
                  databaseEnabled: true,
                  clearCache: false,
                  clearSessionCache: false,
                  sharedCookiesEnabled: true,
                  incognito: false,
                  // Media & Camera (required for QR scanning via getUserMedia)
                  mediaPlaybackRequiresUserGesture: false,
                  allowsInlineMediaPlayback: true,
                  // Desktop User-Agent — prevents mobile detection
                  userAgent: _desktopUserAgent,
                  // Prevent zoom
                  supportZoom: false,
                  builtInZoomControls: false,
                  // Prevent iOS from opening App Store links
                  javaScriptCanOpenWindowsAutomatically: false,
                  // iOS-specific: allow cross-origin requests
                  useShouldOverrideUrlLoading: true,
                  // Dark mode
                  forceDark: AppTheme.current.isDark ? ForceDark.ON : ForceDark.OFF,
                  // Transparent background
                  transparentBackground: true,
                  // Scroll settings
                  overScrollMode: OverScrollMode.ALWAYS,
                ),
                // Inject scripts early — before page fully loads
                initialUserScripts: UnmodifiableListView([
                  UserScript(
                    source: '''
                      // Block WhatsApp/Telegram from redirecting to app stores
                      window.addEventListener('DOMContentLoaded', function() {
                        var banners = document.querySelectorAll('[class*="download"], [class*="app-store"], [class*="native-app"]');
                        banners.forEach(function(b) { b.remove(); });
                      });
                    ''',
                    injectionTime: UserScriptInjectionTime.AT_DOCUMENT_START,
                  ),
                  UserScript(
                    source: '''
                      // MutationObserver: re-apply targeted scroll fixes when SPA mutates the DOM
                      (function() {
                        var _introvertStyle = null;
                        var _introvertObserver = null;

                        function _applyScrollFixes() {
                          // Only act after login
                          var isLoggedIn = document.querySelector('[data-testid="chat-list"]') ||
                                           document.querySelector('.chat-list') ||
                                           document.querySelector('[data-testid="conversation-panel"]') ||
                                           document.querySelector('.im_dialogs_col') ||
                                           document.querySelector('.channels');
                          if (!isLoggedIn) return;

                          // Notify Flutter
                          window.flutter_inappwebview?.callHandler('onLoginDetected', 'true');

                          // Hide bottom chrome — cosmetic only, does not affect scroll
                          if (!_introvertStyle) {
                            _introvertStyle = document.createElement('style');
                            _introvertStyle.textContent = `
                              footer, [class*="bottom-bar"], [class*="bottom-nav"], [class*="tab-bar"],
                              [data-testid="bottom-bar"],
                              .chat-input, .im_history_typing_wrap,
                              .bottom-menu
                              { display: none !important; }
                              body { padding-bottom: 0 !important; }
                            `;
                            document.head.appendChild(_introvertStyle);
                          }

                          // TARGETED scroll fixes on internal containers only — NOT html/body/#app
                          // WhatsApp Web internal scroll containers
                          var waContainers = [
                            '[data-testid="conversation-panel-messages"]',
                            '[data-testid="msg-list"]',
                            '[data-testid="chat-list"]',
                            '._ajxf', '._ak1l', '._ak1o',
                            '.message-list'
                          ];
                          // Telegram Web internal scroll containers
                          var tgContainers = [
                            '.im_history_wrap', '.im_history', '.bubble_list',
                            '.bubbles-inner', '.scrollable', '.messages-container',
                            '.chat-messages', '.im_dialogs_col', '.chat-list'
                          ];
                          var allSelectors = waContainers.concat(tgContainers);
                          allSelectors.forEach(function(sel) {
                            document.querySelectorAll(sel).forEach(function(el) {
                              el.style.setProperty('overflow-y', 'scroll', 'important');
                              el.style.setProperty('-webkit-overflow-scrolling', 'touch', 'important');
                            });
                          });
                        }

                        // Apply once on DOMContentLoaded
                        document.addEventListener('DOMContentLoaded', _applyScrollFixes);

                        // MutationObserver: re-apply when React re-renders the DOM
                        document.addEventListener('DOMContentLoaded', function() {
                          _introvertObserver = new MutationObserver(function(mutations) {
                            // Debounce — only re-apply if childList changed (not every attribute)
                            var needsFix = mutations.some(function(m) { return m.type === 'childList'; });
                            if (needsFix) _applyScrollFixes();
                          });
                          _introvertObserver.observe(document.body, { childList: true, subtree: true });
                        });
                      })();
                    ''',
                    injectionTime: UserScriptInjectionTime.AT_DOCUMENT_START,
                  ),
                ]),
                // Navigation allowlist — block out-of-scope redirects
                shouldOverrideUrlLoading: (controller, navigationAction) async {
                  final uri = navigationAction.request.url;
                  if (uri == null) return NavigationActionPolicy.ALLOW;
                  final host = uri.host.toLowerCase();
                  final allowed = widget.allowedDomain ?? '';
                  // Allow the target domain and its subdomains
                  if (allowed.isEmpty || host.contains(allowed)) {
                    return NavigationActionPolicy.ALLOW;
                  }
                  // Allow common CDN/auth domains that messengers use
                  if (host.contains('whatsapp') || host.contains('telegram') ||
                      host.contains('facebook') || host.contains('fbcdn') ||
                      host.contains('google') || host.contains('gstatic')) {
                    return NavigationActionPolicy.ALLOW;
                  }
                  // Block everything else — open in system browser
                  if (await canLaunchUrl(uri)) {
                    await launchUrl(uri, mode: LaunchMode.externalApplication);
                  }
                  return NavigationActionPolicy.CANCEL;
                },
                // Permission handling — camera for QR scanning, mic for voice notes
                onPermissionRequest: (controller, request) async {
                  return PermissionResponse(
                    resources: request.resources,
                    action: PermissionResponseAction.GRANT,
                  );
                },
                onWebViewCreated: (controller) {
                  _webViewController = controller;
                  // Listen for login detection from injected scripts
                  controller.addJavaScriptHandler(
                    handlerName: 'onLoginDetected',
                    callback: (args) {
                      if (args.isNotEmpty && args[0] == 'true' && mounted) {
                        setState(() {
                          _showSetupGuide = false;
                          _hasCheckedLoginState = true;
                        });
                      }
                    },
                  );
                },
                onLoadStart: (controller, url) {
                  setState(() {
                    _isLoading = true;
                    _currentUrl = url?.toString();
                  });
                },
                onLoadStop: (controller, url) {
                  setState(() {
                    _isLoading = false;
                    _currentUrl = url?.toString();
                  });
                  // Check if user is already logged in and skip setup guide
                  if (!_hasCheckedLoginState) {
                    controller.evaluateJavascript(source: '''
                      (function() {
                        var isLoggedIn = document.querySelector('[data-testid="chat-list"]') ||
                                         document.querySelector('.chat-list') ||
                                         document.querySelector('[data-testid="conversation-panel"]') ||
                                         document.querySelector('.im_dialogs_col') ||
                                         document.querySelector('.channels') ||
                                         document.querySelector('[data-testid="chatlist"]') ||
                                         document.querySelector('.chat_list');
                        return isLoggedIn ? 'true' : 'false';
                      })();
                    ''').then((result) {
                      if (result == 'true' && mounted) {
                        setState(() {
                          _showSetupGuide = false;
                          _hasCheckedLoginState = true;
                        });
                      }
                    });
                  }
                  // Re-inject nav bar hiding on every page load — only after login
                  // (MutationObserver handles SPA re-renders, this catches full navigations)
                  controller.evaluateJavascript(source: '''
                    (function() {
                      var isLoggedIn = document.querySelector('[data-testid="chat-list"]') ||
                                       document.querySelector('.chat-list') ||
                                       document.querySelector('[data-testid="conversation-panel"]') ||
                                       document.querySelector('.im_dialogs_col') ||
                                       document.querySelector('.channels');
                       if (isLoggedIn) {
                        // Hide bottom chrome
                        if (!document.getElementById('introvert-hide-chrome')) {
                          var style = document.createElement('style');
                          style.id = 'introvert-hide-chrome';
                          style.textContent = `
                            footer, [class*="bottom-bar"], [class*="bottom-nav"], [class*="tab-bar"],
                            [data-testid="bottom-bar"],
                            .chat-input, .im_history_typing_wrap,
                            .bottom-menu
                            { display: none !important; }
                            body { padding-bottom: 0 !important; }
                          `;
                          document.head.appendChild(style);
                        }
                        // Targeted scroll fixes on internal containers only
                        var selectors = [
                          '[data-testid="conversation-panel-messages"]',
                          '[data-testid="msg-list"]',
                          '[data-testid="chat-list"]',
                          '._ajxf', '._ak1l', '._ak1o',
                          '.message-list',
                          '.im_history_wrap', '.im_history', '.bubble_list',
                          '.bubbles-inner', '.scrollable', '.messages-container',
                          '.chat-messages', '.im_dialogs_col', '.chat-list'
                        ];
                        selectors.forEach(function(sel) {
                          document.querySelectorAll(sel).forEach(function(el) {
                            el.style.setProperty('overflow-y', 'scroll', 'important');
                            el.style.setProperty('-webkit-overflow-scrolling', 'touch', 'important');
                          });
                        });
                      }
                    })();
                  ''');
                  // Site-specific optimizations
                  _injectSiteSpecificScript(controller);
                },
                onProgressChanged: (controller, progress) {
                  setState(() {
                    _loadProgress = progress / 100.0;
                    if (progress == 100) _isLoading = false;
                  });
                },
                onTitleChanged: (controller, title) {
                  _parseUnreadCount(title ?? '');
                },
                onConsoleMessage: (controller, consoleMessage) {},
              ),
              // Loading progress bar
                if (_isLoading)
                  Positioned(
                    top: 0,
                    left: 0,
                    right: 0,
                    child: LinearProgressIndicator(
                      value: _loadProgress > 0 ? _loadProgress : null,
                      valueColor: AlwaysStoppedAnimation<Color>(widget.accentColor),
                      backgroundColor: widget.accentColor.withOpacity(0.2),
                    ),
                  ),
                // Setup guide overlay (dismissable)
                if (_showSetupGuide) _buildSetupGuide(),
                // Tap-to-reveal navigation bar (top strip)
                // Uses Listener (not GestureDetector) to avoid competing in gesture arena with webview scroll
                Positioned(
                  top: 0,
                  left: 0,
                  right: 0,
                  child: Listener(
                    behavior: HitTestBehavior.translucent,
                    onPointerUp: (event) {
                      setState(() => _showNavBar = !_showNavBar);
                      if (_showNavBar) {
                        Future.delayed(const Duration(seconds: 4), () {
                          if (mounted && _showNavBar) {
                            setState(() => _showNavBar = false);
                          }
                        });
                      }
                    },
                    child: Container(
                      height: 24,
                      color: Colors.transparent,
                      alignment: Alignment.center,
                      child: _showNavBar
                          ? null
                          : Container(
                              width: 36,
                              height: 4,
                              decoration: BoxDecoration(
                                color: AppTheme.current.mutedText.withOpacity(0.3),
                                borderRadius: BorderRadius.circular(2),
                              ),
                            ),
                    ),
                  ),
                ),
                // Navigation bar (shown on tap)
                if (_showNavBar)
                  Positioned(
                    top: 24,
                    left: 0,
                    right: 0,
                    child: _buildTopNavBar(),
                  ),
            ],
          ),
    );
  }

  void _parseUnreadCount(String title) {
    final match = RegExp(r'\((\d+)\)').firstMatch(title);
    final count = match != null ? int.tryParse(match.group(1) ?? '0') ?? 0 : 0;
    if (count != _unreadCount) {
      setState(() => _unreadCount = count);
      widget.onUnreadCountChanged?.call(count);
    }
  }

  void _injectSiteSpecificScript(InAppWebViewController controller) {
    final url = widget.url.toLowerCase();
    if (url.contains('whatsapp.com')) {
      // WhatsApp Web: multiple attempts to get to QR code
      controller.evaluateJavascript(source: '''
        (function() {
          // Remove download/app-store banners and overlays
          var overlays = document.querySelectorAll('[data-testid="intro-md-beta-message"], [data-testid="download-bar"], .overlay, [class*="download"], [class*="banner"]');
          overlays.forEach(function(el) { el.remove(); });

          // Click "Continue to WhatsApp Web" or similar buttons
          var allLinks = document.querySelectorAll('a, button');
          for (var i = 0; i < allLinks.length; i++) {
            var text = (allLinks[i].textContent || '').toLowerCase().trim();
            var href = (allLinks[i].href || '').toLowerCase();
            if ((text.includes('continue') && text.includes('web')) ||
                (text.includes('use') && text.includes('web')) ||
                href.includes('web.whatsapp.com')) {
              allLinks[i].click();
              return;
            }
          }

          // Scroll QR code into view — try multiple selectors
          var qr = document.querySelector('canvas') ||
                   document.querySelector('[data-testid="qrcode"]') ||
                   document.querySelector('div[data-ref]') ||
                   document.querySelector('.qrcode') ||
                   document.querySelector('img[alt*="QR"]');
          if (qr) {
            qr.scrollIntoView({behavior: 'smooth', block: 'center'});
            // Also scroll parent containers
            var parent = qr.parentElement;
            while (parent && parent !== document.body) {
              if (parent.scrollHeight > parent.clientHeight) {
                parent.scrollTop = qr.offsetTop - parent.offsetTop - 50;
              }
              parent = parent.parentElement;
            }
          }

          // Force the page to show desktop layout
          document.querySelector('meta[name="viewport"]')?.setAttribute('content', 'width=1024');
          
          // Targeted scroll fixes on WhatsApp's internal containers only
          var waSelectors = [
            '[data-testid="conversation-panel-messages"]',
            '[data-testid="msg-list"]',
            '[data-testid="chat-list"]',
            '._ajxf', '._ak1l', '._ak1o',
            '.message-list'
          ];
          waSelectors.forEach(function(sel) {
            document.querySelectorAll(sel).forEach(function(el) {
              el.style.setProperty('overflow-y', 'scroll', 'important');
              el.style.setProperty('-webkit-overflow-scrolling', 'touch', 'important');
            });
          });
        })();
      ''');
      // Retry after a delay — WhatsApp loads content dynamically
      Future.delayed(Duration(seconds: 2), () {
        if (mounted) {
          controller.evaluateJavascript(source: '''
            (function() {
              var qr = document.querySelector('canvas') || document.querySelector('[data-testid="qrcode"]');
              if (qr) { qr.scrollIntoView({behavior: 'smooth', block: 'center'}); }
            })();
          ''');
        }
      });
    } else if (url.contains('telegram.org')) {
      controller.evaluateJavascript(source: '''
        (function() {
          var qr = document.querySelector('canvas, .qr-code, .login-qr-image, img[alt*="QR"]');
          if (qr) { qr.scrollIntoView({behavior: 'smooth', block: 'center'}); }
          
          // Targeted scroll fixes on Telegram's internal containers only
          var tgSelectors = [
            '.bubbles-inner', '.scrollable', '.messages-container',
            '.chat-messages', '.im_history_wrap', '.im_history',
            '.bubble_list', '.im_dialogs_col', '.chat-list'
          ];
          tgSelectors.forEach(function(sel) {
            document.querySelectorAll(sel).forEach(function(el) {
              el.style.setProperty('overflow-y', 'scroll', 'important');
              el.style.setProperty('-webkit-overflow-scrolling', 'touch', 'important');
            });
          });
        })();
      ''');
    }
  }

  Widget _buildSetupGuide() {
    return Positioned.fill(
      child: Container(
        color: Colors.black.withOpacity(0.85),
        child: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (!_hasCheckedLoginState) ...[
                    // Show loading while checking login state
                    CircularProgressIndicator(color: widget.accentColor),
                    const SizedBox(height: 16),
                    Text(
                      'Checking ${widget.title} status...',
                      style: TextStyle(color: Colors.white70, fontSize: 14),
                    ),
                  ] else ...[
                    widget.customIcon ?? Icon(widget.icon, color: widget.accentColor, size: 48),
                    const SizedBox(height: 16),
                    Text(
                      'Link Your ${widget.title} Account',
                      style: TextStyle(
                        color: Colors.white,
                        fontSize: 18,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: 20),
                    // Option 1: Phone number
                    Container(
                      padding: const EdgeInsets.all(16),
                      decoration: BoxDecoration(
                        color: Colors.white.withOpacity(0.08),
                        borderRadius: BorderRadius.circular(12),
                        border: Border.all(color: widget.accentColor.withOpacity(0.3)),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Icon(Icons.phone_rounded, color: widget.accentColor, size: 20),
                              const SizedBox(width: 8),
                              Text('Phone Number', style: TextStyle(color: Colors.white, fontSize: 14, fontWeight: FontWeight.w600)),
                              const SizedBox(width: 8),
                              Container(
                                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                decoration: BoxDecoration(
                                  color: widget.accentColor.withOpacity(0.2),
                                  borderRadius: BorderRadius.circular(4),
                                ),
                                child: Text('RECOMMENDED', style: TextStyle(color: widget.accentColor, fontSize: 9, fontWeight: FontWeight.bold)),
                              ),
                            ],
                          ),
                          const SizedBox(height: 8),
                          Text(
                            '1. Tap "Link with phone number instead" on the page below\n'
                            '2. Enter your phone number\n'
                            '3. Enter the verification code you receive',
                            style: TextStyle(color: Colors.white70, fontSize: 12, height: 1.5),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 12),
                    // Option 2: QR code from another device
                    Container(
                      padding: const EdgeInsets.all(16),
                      decoration: BoxDecoration(
                        color: Colors.white.withOpacity(0.05),
                        borderRadius: BorderRadius.circular(12),
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Icon(Icons.qr_code_rounded, color: Colors.white54, size: 20),
                              const SizedBox(width: 8),
                              Text('QR Code from Another Device', style: TextStyle(color: Colors.white, fontSize: 14, fontWeight: FontWeight.w600)),
                            ],
                          ),
                          const SizedBox(height: 8),
                          Text(
                            '1. Take a screenshot of the QR code (power + volume down)\n'
                            '2. Open ${widget.title} on another device (laptop, tablet)\n'
                            '3. Go to Settings → Linked Devices → Scan the screenshot',
                            style: TextStyle(color: Colors.white60, fontSize: 12, height: 1.5),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      'Your chats stay on ${widget.title}\'s servers.\nIntrovert cannot read or access them.',
                      style: TextStyle(color: Colors.white38, fontSize: 10, height: 1.4),
                      textAlign: TextAlign.center,
                    ),
                    const SizedBox(height: 20),
                    ElevatedButton(
                      style: ElevatedButton.styleFrom(
                        backgroundColor: widget.accentColor,
                        foregroundColor: Colors.white,
                        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                        padding: const EdgeInsets.symmetric(horizontal: 32, vertical: 12),
                      ),
                      onPressed: () => setState(() => _showSetupGuide = false),
                      child: Text('Got it', style: TextStyle(fontWeight: FontWeight.w600)),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildTopNavBar() {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      decoration: BoxDecoration(
        color: AppTheme.current.surface.withOpacity(0.95),
        border: Border(
          bottom: BorderSide(color: AppTheme.current.text.withOpacity(0.1), width: 0.5),
        ),
      ),
      child: SafeArea(
        bottom: false,
        child: Row(
          mainAxisAlignment: MainAxisAlignment.spaceEvenly,
          children: [
            _buildBarButton(
              icon: Icons.arrow_back_rounded,
              label: 'Back',
              onTap: () async {
                if (await _webViewController?.canGoBack() ?? false) {
                  _webViewController?.goBack();
                }
              },
            ),
            _buildBarButton(
              icon: Icons.arrow_forward_rounded,
              label: 'Forward',
              onTap: () async {
                if (await _webViewController?.canGoForward() ?? false) {
                  _webViewController?.goForward();
                }
              },
            ),
            _buildBarButton(
              icon: Icons.refresh_rounded,
              label: 'Refresh',
              onTap: () => _webViewController?.reload(),
            ),
            _buildBarButton(
              icon: Icons.home_rounded,
              label: 'Home',
              onTap: () => _webViewController?.loadUrl(
                urlRequest: URLRequest(url: WebUri(widget.url)),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBarButton({
    required IconData icon,
    required String label,
    required VoidCallback? onTap,
  }) {
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, color: AppTheme.current.mutedText, size: 20),
            const SizedBox(height: 2),
            Text(label, style: TextStyle(color: AppTheme.current.mutedText, fontSize: 10)),
          ],
        ),
      ),
    );
  }

  /// Clear all cookies and session data for this WebView
  Future<void> clearSessionData() async {
    await CookieManager.instance().deleteAllCookies();
    await _webViewController?.clearCache();
    if (mounted) {
      setState(() => _showSetupGuide = true);
      _webViewController?.loadUrl(urlRequest: URLRequest(url: WebUri(widget.url)));
    }
  }
}
