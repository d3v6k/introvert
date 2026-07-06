class File {
  const File._();

  String get path => throw UnsupportedError(
      'dart:io File is not available on this platform. Use PdfRenderView.asset, .network, or .memory instead.');
}
