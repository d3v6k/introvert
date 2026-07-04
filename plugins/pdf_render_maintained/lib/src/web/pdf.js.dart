import 'dart:typed_data';

import '../wrappers/html.dart' as html;
import '../wrappers/js_util.dart' as js_util;

Map<String, dynamic> _mergeOptions(
  Map<String, dynamic> params,
  Object? pdfRenderOptions,
) {
  final merged = <String, dynamic>{...params};
  if (pdfRenderOptions != null) {
    final cMapUrl = js_util.getProperty<Object?>(pdfRenderOptions, 'cMapUrl');
    final cMapPacked =
        js_util.getProperty<Object?>(pdfRenderOptions, 'cMapPacked');
    if (cMapUrl != null) merged['cMapUrl'] = cMapUrl;
    if (cMapPacked != null) merged['cMapPacked'] = cMapPacked;

    final otherParams =
        js_util.getProperty<Object?>(pdfRenderOptions, 'params') as Map?;
    if (otherParams != null) {
      merged.addAll(otherParams.cast<String, dynamic>());
    }
  }
  return merged;
}

Future<PdfjsDocument> _loadDocument(Map<String, dynamic> params) async {
  final pdfjsLib = js_util.getProperty<Object?>(html.window, 'pdfjsLib');
  if (pdfjsLib == null) {
    throw Exception('pdfjsLib not found. Ensure pdf.js is loaded on the page.');
  }
  final options = js_util.getProperty<Object?>(html.window, 'pdfRenderOptions');
  final task = js_util.callMethod<Object>(
        pdfjsLib,
        'getDocument',
        [js_util.jsify(_mergeOptions(params, options))],
      ) ??
      (throw Exception('pdfjsLib.getDocument returned null.'));
  final promise = js_util.getProperty<Object?>(task, 'promise');
  if (promise == null) {
    throw Exception('pdfjsLib.getDocument returned an invalid task.');
  }
  final doc = await js_util.promiseToFuture<Object>(promise);
  return PdfjsDocument(doc);
}

Future<PdfjsDocument> pdfjsGetDocument(String url) =>
    _loadDocument({'url': url});

Future<PdfjsDocument> pdfjsGetDocumentFromData(ByteBuffer data) =>
    _loadDocument({'data': data});

class PdfjsDocument {
  PdfjsDocument(this._obj);
  final Object _obj;

  Object getPage(int pageNumber) =>
      js_util.callMethod<Object>(_obj, 'getPage', [pageNumber])!;

  int get numPages => js_util.getProperty<int>(_obj, 'numPages') ?? 0;

  void destroy() => js_util.callMethod<void>(_obj, 'destroy', []);
}

class PdfjsPage {
  PdfjsPage(this._obj);
  final Object _obj;

  PdfjsViewport getViewport(PdfjsViewportParams params) => PdfjsViewport(
        js_util.callMethod<Object>(
          _obj,
          'getViewport',
          [params.toJs()],
        )!,
      );

  PdfjsRender render(PdfjsRenderContext params) => PdfjsRender(
        js_util.callMethod<Object>(
          _obj,
          'render',
          [params.toJs()],
        )!,
      );

  int get pageNumber => js_util.getProperty<int>(_obj, 'pageNumber') ?? 0;

  List<double> get view {
    final raw = js_util.getProperty<Object?>(_obj, 'view');
    if (raw is List) {
      return raw.map((e) => (e as num).toDouble()).toList();
    }
    return const [];
  }
}

class PdfjsViewportParams {
  PdfjsViewportParams({
    required this.scale,
    this.rotation = 0,
    this.offsetX = 0,
    this.offsetY = 0,
    this.dontFlip = false,
  });

  double scale;
  int rotation;
  double offsetX;
  double offsetY;
  bool dontFlip;

  Object toJs() => js_util.jsify({
        'scale': scale,
        'rotation': rotation,
        'offsetX': offsetX,
        'offsetY': offsetY,
        'dontFlip': dontFlip,
      });
}

class PdfjsViewport {
  PdfjsViewport(this._obj);
  final Object _obj;

  double get width =>
      (js_util.getProperty<Object?>(_obj, 'width') as num?)?.toDouble() ?? 0;
  double get height =>
      (js_util.getProperty<Object?>(_obj, 'height') as num?)?.toDouble() ?? 0;

  List<double>? get transform {
    final raw = js_util.getProperty<Object?>(_obj, 'transform');
    if (raw is List) {
      return raw.map((e) => (e as num).toDouble()).toList();
    }
    return null;
  }
}

class PdfjsRenderContext {
  PdfjsRenderContext({
    required this.canvasContext,
    required this.viewport,
    this.intent = 'display',
    this.renderInteractiveForms = false,
    this.transform,
    this.imageLayer,
    this.canvasFactory,
    this.background,
  });

  html.CanvasRenderingContext2D canvasContext;
  PdfjsViewport viewport;
  String intent;
  bool renderInteractiveForms;
  List<double>? transform;
  dynamic imageLayer;
  dynamic canvasFactory;
  dynamic background;

  Object toJs() => js_util.jsify({
        'canvasContext': canvasContext,
        'viewport': viewport._obj,
        'intent': intent,
        'renderInteractiveForms': renderInteractiveForms,
        if (transform != null) 'transform': transform,
        if (imageLayer != null) 'imageLayer': imageLayer,
        if (canvasFactory != null) 'canvasFactory': canvasFactory,
        if (background != null) 'background': background,
      });
}

class PdfjsRender {
  PdfjsRender(this._obj);
  final Object _obj;

  Object get promise => js_util.getProperty<Object>(_obj, 'promise')!;
}
