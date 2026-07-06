import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../pdf_render_widgets.dart' show PdfViewerParams;
import '_internal/file_stub.dart' if (dart.library.io) '_internal/io_file.dart';
import 'pdf_render_view.dart';

/// A batteries-included screen that wires an [AppBar] to a [PdfRenderView].
class PdfRenderScreen extends StatelessWidget {
  const PdfRenderScreen({
    super.key,
    required this.viewer,
    this.title = 'PDF Viewer',
  });

  final Widget viewer;
  final String title;

  factory PdfRenderScreen.asset(
    String assetPath, {
    String title = 'PDF Viewer',
    PdfViewerParams? params,
    Key? key,
  }) =>
      PdfRenderScreen(
        key: key,
        title: title,
        viewer: PdfRenderView.asset(assetPath, params: params),
      );

  factory PdfRenderScreen.file(
    File file, {
    String title = 'PDF Viewer',
    PdfViewerParams? params,
    Key? key,
  }) =>
      PdfRenderScreen(
        key: key,
        title: title,
        viewer: PdfRenderView.file(file, params: params),
      );

  factory PdfRenderScreen.network(
    String url, {
    String title = 'PDF Viewer',
    PdfViewerParams? params,
    Key? key,
  }) =>
      PdfRenderScreen(
        key: key,
        title: title,
        viewer: PdfRenderView.network(url, params: params),
      );

  factory PdfRenderScreen.memory(
    Uint8List bytes, {
    String title = 'PDF Viewer',
    PdfViewerParams? params,
    Key? key,
  }) =>
      PdfRenderScreen(
        key: key,
        title: title,
        viewer: PdfRenderView.memory(bytes, params: params),
      );

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: Text(title)),
        body: SafeArea(child: viewer),
      );
}
