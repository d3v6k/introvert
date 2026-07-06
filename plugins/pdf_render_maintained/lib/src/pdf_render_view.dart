import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../pdf_render_widgets.dart';
import '_internal/file_stub.dart' if (dart.library.io) '_internal/io_file.dart';

/// High-level 1-line PDF viewer that delegates to the existing [PdfViewer] API.
class PdfRenderView extends StatelessWidget {
  const PdfRenderView._(
    this._source, {
    this.params,
    String? assetPath,
    String? networkUrl,
    File? file,
    Uint8List? bytes,
    super.key,
  })  : _assetPath = assetPath,
        _networkUrl = networkUrl,
        _file = file,
        _bytes = bytes;

  const PdfRenderView.asset(
    String assetPath, {
    PdfViewerParams? params,
    Key? key,
  }) : this._(
          const _PdfSource.asset(),
          params: params,
          assetPath: assetPath,
          key: key,
        );

  const PdfRenderView.file(
    File file, {
    PdfViewerParams? params,
    Key? key,
  }) : this._(
          const _PdfSource.file(),
          params: params,
          file: file,
          key: key,
        );

  const PdfRenderView.network(
    String url, {
    PdfViewerParams? params,
    Key? key,
  }) : this._(
          const _PdfSource.network(),
          params: params,
          networkUrl: url,
          key: key,
        );

  const PdfRenderView.memory(
    Uint8List bytes, {
    PdfViewerParams? params,
    Key? key,
  }) : this._(
          const _PdfSource.memory(),
          params: params,
          bytes: bytes,
          key: key,
        );

  final _PdfSource _source;
  final PdfViewerParams? params;
  final String? _assetPath;
  final String? _networkUrl;
  final File? _file;
  final Uint8List? _bytes;

  @override
  Widget build(BuildContext context) => _source.buildViewer(
        params,
        assetPath: _assetPath,
        networkUrl: _networkUrl,
        file: _file,
        bytes: _bytes,
      );
}

enum _PdfSourceType { asset, file, network, memory }

class _PdfSource {
  const _PdfSource._(this.type);

  const _PdfSource.asset() : this._(_PdfSourceType.asset);

  const _PdfSource.file() : this._(_PdfSourceType.file);

  const _PdfSource.network() : this._(_PdfSourceType.network);

  const _PdfSource.memory() : this._(_PdfSourceType.memory);

  final _PdfSourceType type;

  Widget buildViewer(
    PdfViewerParams? params, {
    String? assetPath,
    String? networkUrl,
    File? file,
    Uint8List? bytes,
  }) {
    switch (type) {
      case _PdfSourceType.asset:
        return PdfViewer.openAsset(assetPath!, params: params);
      case _PdfSourceType.file:
        return PdfViewer.openFile(file!.path, params: params);
      case _PdfSourceType.network:
        return PdfViewer.openFutureData(
          () => _loadNetworkData(networkUrl!),
          params: params,
        );
      case _PdfSourceType.memory:
        return PdfViewer.openData(bytes!, params: params);
    }
  }

  Future<Uint8List> _loadNetworkData(String url) async {
    final uri = Uri.parse(url);
    final byteData = await NetworkAssetBundle(uri).load(url);
    return byteData.buffer.asUint8List();
  }
}
