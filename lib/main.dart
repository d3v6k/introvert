import 'package:path_provider/path_provider.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'dart:io';
import 'dart:typed_data';
import 'package:shared_preferences/shared_preferences.dart';
import 'src/native/introvert_client.dart';
import 'src/native/identity_manager.dart';
import 'src/native/alert_service.dart';
import 'src/ui/main_shell.dart';
import 'src/ui/onboarding_screen.dart';
import 'src/repository/sync_repository.dart';
import 'theme/app_theme.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await AppTheme.current.loadTheme();

  final client = IntrovertClient();
  
  // Resolve sandbox directories immediately so path translations work correctly
  try {
    final supportDir = await getApplicationSupportDirectory();
    final docsDir = await getApplicationDocumentsDirectory();
    client.initSandboxPaths(supportDir.path, docsDir.path);
  } catch (e) {
    debugPrint("Failed to initialize sandbox directories: $e");
  }

  final idManager = IdentityManager();
  final syncRepository = SyncRepository();

  runApp(
    MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => SyncStateNotifier(syncRepository)),
        Provider<IntrovertClient>.value(value: client),
        Provider<IdentityManager>.value(value: idManager),
      ],
      child: const IntrovertApp(),
    ),
  );
}

class IntrovertApp extends StatefulWidget {
  const IntrovertApp({super.key});

  @override
  State<IntrovertApp> createState() => _IntrovertAppState();
}

class _IntrovertAppState extends State<IntrovertApp> {
  bool _isLoading = true;
  bool _showOnboarding = false;
  String? _dbPath;
  final GlobalKey<ScaffoldMessengerState> _messengerKey = GlobalKey<ScaffoldMessengerState>();

  @override
  void initState() {
    super.initState();
    AlertService.initialize();
    _initialize();
  }

  Future<void> _initialize() async {
    debugPrint("⏳ Starting sovereign initialization sequence...");
    final idManager = IdentityManager();
    final client = IntrovertClient();
    
    try {
            if (Platform.isAndroid || Platform.isMacOS || Platform.isIOS) {
        final dir = await getApplicationSupportDirectory();
        if (!await dir.exists()) await dir.create(recursive: true);
        _dbPath = "${dir.path}/introvert.db";
        debugPrint("📍 Target DB Path (Sandboxed): $_dbPath");
        debugPrint("📍 Target DB Path (Apple/Sanboxed): $_dbPath");
      } else {
        _dbPath = "./introvert.db";
      }
      
      debugPrint("🔑 Checking for existing identity...");
      final existingSeed = await idManager.getSeed();
      
      if (existingSeed != null) {
        debugPrint("🧠 Identity found. Starting native engine...");
        // Non-blocking delay to allow UI to breathe
        await Future.delayed(const Duration(milliseconds: 500));
        
        try {
          client.startEngine(existingSeed, _dbPath!);
        } catch (e) {
          debugPrint("🚨 Engine failed on existing DB. Attempting forced reset...");
          final file = File(_dbPath!);
          if (await file.exists()) await file.delete();
          client.startEngine(existingSeed, _dbPath!);
        }
        debugPrint("📡 Starting networking plane...");
        client.startNetwork();
        debugPrint("🚀 Introvert Engine Started Successfully!");
        
        // Restore saved Anchor Mode settings
        try {
          final prefs = await SharedPreferences.getInstance();
          final isAnchorMode = prefs.getBool('isAnchorMode') ?? false;
          if (isAnchorMode) {
            debugPrint("⚓ Restoring saved Anchor Mode setting...");
            client.setAnchorMode(true);
            AlertService.setStayAwake(true);
          }
        } catch (e) {
          debugPrint("⚠️ Failed to restore Anchor Mode setting: $e");
        }
      } else {
        debugPrint("🆕 No identity found. Transitioning to onboarding.");
        _showOnboarding = true;
      }
    } catch (e) {
      debugPrint("❌ Initialization Error: $e");
      _showOnboarding = true;
    } finally {
      if (mounted) {
        debugPrint("✅ Initialization complete. Loading UI.");
        setState(() {
          _isLoading = false;
        });
      }
    }
  }

  void _onOnboardingComplete(Uint8List seed, String avatarName) async {
    final client = Provider.of<IntrovertClient>(context, listen: false);
    
    try {
      if (_dbPath == null) throw Exception("Database path not initialized");
      try {
        client.startEngine(seed, _dbPath!);
      } catch (e) {
        debugPrint("🚨 Onboarding: Engine failed to start on existing DB. Resetting...");
        final file = File(_dbPath!);
        if (await file.exists()) await file.delete();
        client.startEngine(seed, _dbPath!);
      }
      client.startNetwork();
      
      // Save Avatar Name (privacy_mode=1: allow unknown users to connect by default)
      client.setProfile(avatarName, null, null, 1);
      
      // Restore saved Anchor Mode settings
      SharedPreferences.getInstance().then((prefs) {
        final isAnchorMode = prefs.getBool('isAnchorMode') ?? false;
        if (isAnchorMode) {
          client.setAnchorMode(true);
          AlertService.setStayAwake(true);
        }
      }).catchError((_) {});

      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) {
          setState(() => _showOnboarding = false);
        }
      });
    } catch (e) {
      _messengerKey.currentState?.showSnackBar(
        SnackBar(content: Text('Engine failed to start: $e')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: AppTheme.current,
      builder: (context, _) {
        final theme = AppTheme.current.theme;
        final brightness = theme.bg.computeLuminance() > 0.5 ? Brightness.light : Brightness.dark;
        
        return MaterialApp(
          scaffoldMessengerKey: _messengerKey,
          title: 'Introvert P2P',
          debugShowCheckedModeBanner: false,
          theme: ThemeData(
            useMaterial3: true,
            brightness: brightness,
            scaffoldBackgroundColor: theme.bg,
            primaryColor: theme.accent,
            colorScheme: ColorScheme.fromSeed(
              seedColor: theme.accent,
              brightness: brightness,
              primary: theme.accent,
              surface: theme.surface,
              onSurface: theme.text,
              secondary: theme.accent,
            ).copyWith(
               surface: theme.surface,
               onSurface: theme.text,
            ),
            textTheme: Typography.material2021(platform: Theme.of(context).platform).black.apply(
              bodyColor: theme.text,
              displayColor: theme.text,
            ),
            iconTheme: IconThemeData(color: theme.accent),
            cardTheme: CardThemeData(
              color: theme.surface,
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(12),
                side: BorderSide(color: theme.mutedText.withValues(alpha: 0.1)),
              ),
            ),
            appBarTheme: AppBarTheme(
              backgroundColor: theme.surface,
              foregroundColor: theme.text,
              elevation: 0,
              iconTheme: IconThemeData(color: theme.text),
            ),
            navigationBarTheme: NavigationBarThemeData(
              backgroundColor: theme.surface,
              indicatorColor: theme.accent.withValues(alpha: 0.2),
              labelTextStyle: WidgetStateProperty.all(
                TextStyle(fontSize: 12, fontWeight: FontWeight.w500, color: theme.text),
              ),
              iconTheme: WidgetStateProperty.resolveWith((states) {
                if (states.contains(WidgetState.selected)) {
                  return IconThemeData(color: theme.accent);
                }
                return IconThemeData(color: theme.mutedText);
              }),
            ),
          ),
          home: _isLoading
              ? Scaffold(
                  backgroundColor: theme.bg,
                  body: Center(
                    child: CircularProgressIndicator(color: theme.accent),
                  ),
                )
              : (_showOnboarding
                  ? OnboardingScreen(onComplete: _onOnboardingComplete, messengerKey: _messengerKey)
                  : const MainShell()),
        );
      }
    );
  }
}
