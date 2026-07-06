import 'package:flutter/material.dart';
import 'package:flutter_markdown/flutter_markdown.dart';
import '../../theme/app_theme.dart';

/// Blocking Terms of Use screen shown on first launch.
/// The "Agree & Continue" button activates when the user checks the agreement checkbox.
/// The "Don't show again" checkbox is optional.
class TermsScreen extends StatefulWidget {
  final VoidCallback onAccepted;

  const TermsScreen({super.key, required this.onAccepted});

  @override
  State<TermsScreen> createState() => _TermsScreenState();
}

class _TermsScreenState extends State<TermsScreen> {
  bool _dontShowAgain = false;
  bool _agreedToTerms = false;

  bool get _canContinue => _agreedToTerms;

  static const String _termsText = r'''
# Introvert App — Terms of Use & Liability Disclaimer

**Last Updated: June 2026**

---

## ⚠️ CRITICAL NOTICE FOR ALL USERS

Introvert is **not a service**. It is a decentralized, open-source, serverless peer-to-peer (P2P) communication software utility. By utilizing this software, you operate your own autonomous node within a distributed mesh network. The creators, developers, and maintainers of Introvert have **no control over the network**, hold **zero user data**, and have **no capacity to monitor, moderate, or log your traffic**.

---

## 1. Legal Status of the Software

Introvert is an open-source communication software tool, completely separate from any financial or asset-management services.

- **Not a Wallet:** Introvert does not hold, store, transmit, or facilitate the transfer of any funds, cryptocurrencies, or digital assets. It completely lacks the programming or capacity to handle peer-to-peer financial transactions.

- **External Token Management:** Any interaction with the Solana blockchain or the management of your $INTR tokens must occur exclusively outside of this application using third-party software.

- **No Central Infrastructure:** There are no central databases, cloud platforms, or authentication servers. Your cryptographic identity is derived entirely on your local device using an offline 12-word BIP-39 mnemonic phrase.

---

## 2. Age Restriction & Eligibility

- **Minimum Age:** You must be at least 18 years of age (or the legal age of majority within your sovereign jurisdiction) to open, execute, or interact with this software.

- **Representation of Age:** By checking the agreement box and running this software, you explicitly represent and warrant that you meet this age requirement. If you are under 18, you are strictly prohibited from using Introvert.

---

## 3. Absolute Prohibition of Illegal & Covert Activities

Because Introvert operates on total user sovereignty, you bear exclusive legal liability for all content, data, and metadata you route through your node.

- **Lawful Use Only:** You explicitly agree to use this software solely for lawful intents and activities.

- **Prohibited Actions:** You are strictly forbidden from utilizing Introvert to facilitate, execute, or conceal criminal enterprise, malicious hacking, harassment, or the distribution, transmission, or caching of any illicit material.

- **Zero Indemnification:** You acknowledge that the open-source developers will not protect, legally defend, or indemnify you if you use this utility to violate local or international laws.

---

## 4. Complete Disclaimer of Warranties & Limitation of Liability

- **Provided "As Is":** This open-source software code is provided entirely "as is" and "as available," without warranties of any scale, express or implied.

- **Liability Cap:** Under no legal theory or jurisdiction shall the developers, contributors, or maintainers of the Introvert ecosystem be liable for any direct, indirect, incidental, or consequential damages, leaks, hardware wear, or data loss resulting from your use or inability to use the codebase. You proceed entirely at your own risk.

- **Data and Identity Loss:** You are uniquely responsible for backing up your BIP-39 seed phrase. There is no central password recovery mechanism; if you lose your seed phrase, you permanently lose your network identity and local database access.
''';

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      body: SafeArea(
        child: Column(
          children: [
            // Header
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 24, 24, 12),
              child: Row(
                children: [
                  Icon(Icons.gavel_rounded, color: AppTheme.current.accent, size: 28),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      'Terms of Use & Disclaimer',
                      style: TextStyle(
                        color: AppTheme.current.text,
                        fontSize: 20,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.2), height: 1),
            // Scrollable terms
            Expanded(
              child: Markdown(
                data: _termsText,
                padding: const EdgeInsets.all(24),
                styleSheet: MarkdownStyleSheet(
                  h1: TextStyle(color: AppTheme.current.text, fontSize: 20, fontWeight: FontWeight.bold),
                  h2: TextStyle(color: AppTheme.current.accent, fontSize: 16, fontWeight: FontWeight.bold),
                  h3: TextStyle(color: AppTheme.current.text, fontSize: 14, fontWeight: FontWeight.w600),
                  p: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.85), fontSize: 13, height: 1.5),
                  listBullet: TextStyle(color: AppTheme.current.accent, fontSize: 13),
                  strong: TextStyle(color: AppTheme.current.text, fontWeight: FontWeight.bold),
                  blockquote: TextStyle(color: AppTheme.current.mutedText, fontStyle: FontStyle.italic),
                  horizontalRuleDecoration: BoxDecoration(
                    border: Border(top: BorderSide(color: AppTheme.current.mutedText.withValues(alpha: 0.2))),
                  ),
                ),
              ),
            ),
            Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.2), height: 1),
            // Checkboxes and button
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 16, 24, 24),
              child: Column(
                children: [
                  // Checkbox 1: Don't show again
                  InkWell(
                    onTap: () => setState(() => _dontShowAgain = !_dontShowAgain),
                    borderRadius: BorderRadius.circular(8),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: Row(
                        children: [
                          SizedBox(
                            width: 24,
                            height: 24,
                            child: Checkbox(
                              value: _dontShowAgain,
                              onChanged: (v) => setState(() => _dontShowAgain = v ?? false),
                              activeColor: AppTheme.current.accent,
                              side: BorderSide(color: AppTheme.current.mutedText),
                            ),
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Text(
                              "Don't show this again",
                              style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 4),
                  // Checkbox 2: Agree to terms
                  InkWell(
                    onTap: () => setState(() => _agreedToTerms = !_agreedToTerms),
                    borderRadius: BorderRadius.circular(8),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: Row(
                        children: [
                          SizedBox(
                            width: 24,
                            height: 24,
                            child: Checkbox(
                              value: _agreedToTerms,
                              onChanged: (v) => setState(() => _agreedToTerms = v ?? false),
                              activeColor: AppTheme.current.accent,
                              side: BorderSide(color: AppTheme.current.mutedText),
                            ),
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: Text(
                              "I have read, understood, and agree to the Terms of Use",
                              style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  // Continue button
                  SizedBox(
                    width: double.infinity,
                    height: 48,
                    child: ElevatedButton(
                      onPressed: _canContinue ? widget.onAccepted : null,
                      style: ElevatedButton.styleFrom(
                        backgroundColor: AppTheme.current.accent,
                        foregroundColor: AppTheme.current.bg,
                        disabledBackgroundColor: AppTheme.current.mutedText.withValues(alpha: 0.2),
                        disabledForegroundColor: AppTheme.current.mutedText,
                        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                      ),
                      child: Text(
                        'Agree & Continue',
                        style: TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.bold,
                          color: _canContinue ? AppTheme.current.bg : AppTheme.current.mutedText,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
