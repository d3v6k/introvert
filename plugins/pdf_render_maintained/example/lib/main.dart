import 'dart:io';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cache_manager/flutter_cache_manager.dart';
import 'package:pdf_render_maintained/pdf_render_maintained.dart';

void main(List<String> args) => runApp(const PdfRenderExampleApp());

class PdfRenderExampleApp extends StatelessWidget {
  const PdfRenderExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'pdf_render_maintained example',
      theme: ThemeData(
        colorSchemeSeed: Colors.indigo,
        useMaterial3: true,
      ),
      home: const ExampleHome(),
    );
  }
}

class ExampleHome extends StatefulWidget {
  const ExampleHome({super.key});

  static const _samplePdfUrl =
      'https://github.com/espresso3389/flutter_pdf_render/raw/master/example/assets/hello.pdf';

  @override
  State<ExampleHome> createState() => _ExampleHomeState();
}

class _ExampleHomeState extends State<ExampleHome> {
  static const _autoShowAsset = bool.fromEnvironment('AUTO_SHOW_ASSET_VIEW');
  bool _didAutoShow = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_autoShowAsset && !_didAutoShow) {
      _didAutoShow = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        _openAsset(context);
      });
    }
  }

  void _openAsset(BuildContext context) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => PdfRenderScreen.asset(
          'assets/hello.pdf',
          title: 'Asset PDF',
          params: const PdfViewerParams(padding: 12, minScale: 1.0),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('pdf_render_maintained samples')),
      body: ListView(
        children: [
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 24, 16, 12),
            child: Text(
              '1-Line Setup (new)',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
            ),
          ),
          _DemoTile(
            icon: Icons.insert_drive_file,
            title: 'Asset viewer',
            subtitle: 'const PdfRenderView.asset(...)',
            onTap: () => _openAsset(context),
          ),
          _DemoTile(
            icon: Icons.cloud_download,
            title: 'Network viewer',
            subtitle: 'const PdfRenderView.network(...)',
            onTap: () => Navigator.push(
              context,
              MaterialPageRoute(
                builder: (_) => PdfRenderScreen.network(
                  ExampleHome._samplePdfUrl,
                  title: 'Network PDF',
                  params: const PdfViewerParams(padding: 12, minScale: 1.0),
                ),
              ),
            ),
          ),
          _DemoTile(
            icon: Icons.memory,
            title: 'In-memory viewer',
            subtitle: 'const PdfRenderView.memory(...)',
            onTap: () async {
              final data = await rootBundle.load('assets/hello.pdf');
              if (!context.mounted) return;
              Navigator.push(
                context,
                MaterialPageRoute(
                  builder: (_) => PdfRenderScreen.memory(
                    data.buffer.asUint8List(),
                    title: 'In-memory PDF',
                    params: const PdfViewerParams(padding: 12, minScale: 1.0),
                  ),
                ),
              );
            },
          ),
          const Divider(),
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 24, 16, 12),
            child: Text(
              'Advanced APIs',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
            ),
          ),
          _DemoTile(
            icon: Icons.tune,
            title: 'Controller-driven viewer',
            subtitle: 'Legacy PdfViewer.openAsset/openFutureFile sample',
            onTap: () => Navigator.push(
              context,
              MaterialPageRoute(
                builder: (_) => const AdvancedViewerPage(),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _DemoTile extends StatelessWidget {
  const _DemoTile({
    required this.icon,
    required this.title,
    required this.subtitle,
    this.onTap,
  });

  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: Icon(icon),
      title: Text(title),
      subtitle: Text(subtitle),
      trailing: const Icon(Icons.chevron_right),
      onTap: onTap,
    );
  }
}

class AdvancedViewerPage extends StatefulWidget {
  const AdvancedViewerPage({super.key});

  @override
  State<AdvancedViewerPage> createState() => _AdvancedViewerPageState();
}

class _AdvancedViewerPageState extends State<AdvancedViewerPage> {
  final controller = PdfViewerController();
  TapDownDetails? _doubleTapDetails;

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: ValueListenableBuilder<Matrix4>(
          valueListenable: controller,
          builder: (context, _, __) => Text(
            controller.isReady
                ? 'Page #${controller.currentPageNumber}'
                : 'Page -',
          ),
        ),
      ),
      backgroundColor: Colors.grey,
      body: GestureDetector(
        onDoubleTapDown: (details) => _doubleTapDetails = details,
        onDoubleTap: () => controller.ready?.setZoomRatio(
          zoomRatio: controller.zoomRatio * 1.5,
          center: _doubleTapDetails!.localPosition,
        ),
        child: Stack(
          children: [
            !kIsWeb && Platform.isMacOS
                ? PdfViewer.openFutureFile(
                    () async => (await DefaultCacheManager()
                            .getSingleFile(ExampleHome._samplePdfUrl))
                        .path,
                    viewerController: controller,
                    onError: (err) => debugPrint(err.toString()),
                    params: const PdfViewerParams(
                      padding: 10,
                      minScale: 1.0,
                    ),
                  )
                : PdfViewer.openAsset(
                    'assets/hello.pdf',
                    viewerController: controller,
                    onError: (err) => debugPrint(err.toString()),
                    params: const PdfViewerParams(
                      padding: 10,
                      minScale: 1.0,
                    ),
                  ),
            ValueListenableBuilder(
              valueListenable: controller,
              builder: (context, m, child) {
                if (!controller.isReady) return const SizedBox.shrink();
                final v = controller.viewRect;
                final all = controller.fullSize;
                final top = v.top / all.height * v.height;
                final height = v.height / all.height * v.height;
                return Positioned(
                  right: 0,
                  top: top,
                  height: height,
                  width: 8,
                  child: Container(color: Colors.red),
                );
              },
            ),
          ],
        ),
      ),
      floatingActionButton: Column(
        mainAxisAlignment: MainAxisAlignment.end,
        children: <Widget>[
          FloatingActionButton(
            child: const Icon(Icons.first_page),
            onPressed: () => controller.ready?.goToPage(pageNumber: 1),
          ),
          FloatingActionButton(
            child: const Icon(Icons.last_page),
            onPressed: () =>
                controller.ready?.goToPage(pageNumber: controller.pageCount),
          ),
          FloatingActionButton(
            child: const Icon(Icons.bug_report),
            onPressed: () => rendererTest(),
          ),
        ],
      ),
    );
  }

  Future<void> rendererTest() async {
    final PdfDocument doc;
    if (!kIsWeb && Platform.isMacOS) {
      final file =
          (await DefaultCacheManager().getSingleFile(ExampleHome._samplePdfUrl))
              .path;
      doc = await PdfDocument.openFile(file);
    } else {
      doc = await PdfDocument.openAsset('assets/hello.pdf');
    }

    try {
      final page = await doc.getPage(1);
      final image = await page.render();
      debugPrint(
        '${image.width}x${image.height}: ${image.pixels.lengthInBytes} bytes.',
      );
    } finally {
      doc.dispose();
    }
  }
}
