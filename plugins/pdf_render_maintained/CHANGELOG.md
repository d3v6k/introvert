## 1.6.1 (Maintained Fork)

- 🛠️ **iOS/macOS alignment**: Raised iOS minimum to 13 and fixed the Swift bridge import so Xcode builds succeed with the renamed plugin.
- 🖼️ **Docs refresh**: Added an up-to-date iOS screenshot and refreshed README/PUBLISHING_GUIDE for pub.dev/GitHub.
- 🔖 **Version bump**: Published as 1.6.1.

## 1.6.0 (Maintained Fork)

- ✨ **1-Line viewer API**: Added `PdfRenderView` with `.asset`, `.file`, `.network`, and `.memory` constructors that wrap the existing `PdfViewer.open*` factories without breaking changes.
- 🧭 **Batteries-included screen**: Introduced `PdfRenderScreen.*` helpers so developers can push a ready-to-use viewer page with a single line.
- 📘 **Docs & examples**: README now highlights the new API, `example` app showcases both the convenience and advanced flows, and `pdf_render_maintained.dart` exports the new widgets for easy imports.

## 1.5.10 (Maintained Fork)

- ✅ **Doc & release bump**: Updated docs and package version for publishing

## 1.5.9 (Maintained Fork)

- ✅ **Doc refresh**: README/PUBLISHING_GUIDE updates and final publish prep

## 1.5.8 (Maintained Fork)

- ✅ **Publish prep**: Version bump and final adjustments for pub.dev release
- ✅ **iOS SPM manifest**: Added `ios/pdf_render_maintained/Package.swift` for Swift Package Manager detection

## 1.5.7 (Maintained Fork)

- ✅ **WASM-safe bindings**: Rewrote pdf.js interop to use `dart:js_util` helpers and platform-aware wrappers for wasm detection
- ✅ **Dependency cleanup**: Removed discontinued `js` package and updated `web` to the latest stable line
- ✅ **Publish prep**: Trimmed build artifacts and caches to reduce package size

## 1.5.6 (Maintained Fork)

- ✅ **WASM Compatibility Improvements**: Added `package:web` dependency and WASM-compatible HTML wrapper implementation
- ✅ **Enhanced WASM Detection**: Improved conditional exports to use `package:web` for WASM builds

## 1.5.5 (Maintained Fork)

- ✅ **Windows and Linux Platform Support**: Added platform declarations and C++ plugin implementations for Windows and Linux
- ✅ **WASM Compatibility**: Fixed WASM compatibility by adding `dart:js_interop` support with proper conditional exports
- ✅ **Downgrade Analysis Fix**: Added stub implementations to ensure compatibility with dependency constraint lower bounds
- ✅ **Conditional Exports**: Improved conditional export structure for proper WASM/non-WASM web handling
- ✅ **Platform Coverage**: Updated description to include Windows and Linux platforms

## 1.5.4 (Maintained Fork)

- ✅ **Deprecation Fix**: Converted `alignPanAxis` from field to computed getter
- ✅ **Constructor Update**: Updated `PdfViewerParams` constructor to handle `alignPanAxis` deprecation
- ✅ **copyWith Fix**: Fixed `copyWith` method to properly handle `panAxis` and deprecated `alignPanAxis`

## 1.5.3 (Maintained Fork)

- ✅ **Swift Package Manager Support**: Added SPM support for macOS
- ✅ **Package.swift**: Created proper Swift Package Manager configuration
- ✅ **macOS SPM**: Full Swift Package Manager compatibility for macOS

## 1.5.2 (Maintained Fork)

- ✅ **Final Publishing**: Package successfully published to pub.dev
- ✅ **Complete Documentation**: All guides and documentation updated
- ✅ **Version Update**: Bumped to 1.5.2 for final release

## 1.5.1 (Maintained Fork)

- ✅ **WASM Compatibility**: Fixed `dart:ffi` imports with conditional exports for web/WASM compatibility
- ✅ **Gradle 8.11.1 Support**: Updated Gradle wrapper to support modern Android builds
- ✅ **Dependency Updates**: Updated to latest versions (collection ^1.19.1, vector_math ^2.2.0)
- ✅ **Removed Discontinued Package**: Removed `js` package dependency
- ✅ **Static Analysis**: Fixed all linting errors and warnings
- ✅ **Platform Interface**: Updated to use conditional imports for web/WASM
- ✅ **Android Testing**: Verified working on real devices (Pixel 6a) and emulators
- ✅ **Build Compatibility**: All platforms (iOS, Android, macOS) build successfully
- ✅ **Code Quality**: Improved code structure and maintainability
- ✅ **Documentation**: Updated README with latest features and improvements

## 1.5.0 (Maintained Fork)

**This is a maintained fork of the original pdf_render package by espresso3389**

### Breaking Changes
- Package name changed from `pdf_render` to `pdf_render_maintained`
- Android package identifier changed to `com.khokanuzzman.pdf_render_maintained`

### Updates
- Updated all dependencies to latest versions
- Fixed compatibility with Flutter 3.0+ and Dart 3.0+
- Updated SDK constraints to `>=3.0.0 <4.0.0`
- Fixed linter errors and code quality issues
- Updated example project to use new package name
- Added comprehensive documentation with proper attribution

### Dependencies Updated
- `js: ^0.7.2` (from ^0.6.7)
- `flutter_lints: ^6.0.0` (from ^4.0.0)
- `flutter_cache_manager: ^3.4.1` (example project)

### Attribution
This package is based on the excellent work by [espresso3389](https://github.com/espresso3389) and the original [pdf_render](https://pub.dev/packages/pdf_render) package. We maintain this fork to keep the package alive for the Flutter community.

## 1.4.13 (Original Package)

- Modernize the Android toolchain: migrate to the Gradle plugins DSL, update to AGP 8.9.1, Kotlin 2.1.0, and compileSdk 36.
- Adopt Flutter's declarative plugin loader and refresh Gradle wrapper/JVM settings for compatibility with Flutter 3.35.
- Refresh dependencies via `flutter pub upgrade` to restore Android build success on current Flutter stable.

## 1.4.12

- Merge PR #148 to handle click outside page view

## 1.4.11

- Merge PR #152 to set jvmTarget 1.8

## 1.4.9/1.4.10

- FIXED: App fails to compile for macOS (#80)

## 1.4.8

- js plugin backed to 0.6.7
- Fixes memory leak when closing PdfViewer (#147)

## 1.4.7

- Forked InteractiveViewer code to support wheel-to-scroll (PR #144)
- Flutter 3.21 build fix (PR #143)

## 1.4.6

- Minor fixes

## 1.4.5

- `README.md` updates.

## 1.4.4

- `README.md` updates.

## 1.4.3

- New plugin, named [pdfrx](https://pub.dev/packages/pdfrx), is a better replacement for pdf_render. And pdf_render is now in maintenance mode. **No new features are added to pdf_render.**

## 1.4.2

- android/build.gradle update toward Gradle Plugin 8+ (PR #131)

## 1.4.1

- Adding many scroll position related members on PdfViewerController to somehow implement scroll position indicator (#129)

## 1.4.0

- BREAKING CHANGE: `PdfViewerController.getPage` now returns `Future<PdfPage>` (#115)
- Fix PdfViewer may leave pages unrendered when scolled down page by page (#121)

## 1.3.8

- Add `PdfViewerController.getPage` to help rendering on `buildPageOverlay` (#110)

## 1.3.7

- Add `PdfViewerParams.panAxis` (PR #105)

## 1.3.6

- Fixes dependencies to work on stable channel

## 1.3.5

- Merging PR: Fix allowAntialiasingIOS #98

## 1.3.4

- FIXED: await page.render() crashed on web #66 (and add a button to test rendering API on example)

## 1.3.3

- Minor, but PDF rendering result change (for Android/iOS) due to consistency issue described on #88

## 1.3.2

- Minor dependency update on example code.

## 1.3.1

- Fix [#66](https://github.com/espresso3389/flutter_pdf_render/issues/66) await page.render() crashed on web

## 1.3.0

- BREAKING CHANGE: Simpler PdfPageImageTexture structure to improve rendering performance especially on Flutter Web.

## 1.2.1

- Certain Flutter version shows "Error: Type 'Uint8List' not found" though another version does not.

## 1.2.0

- Fix Rendering breakage on recent Flutter ([#84](https://github.com/espresso3389/flutter_pdf_render/issues/84))

## 1.1.0

- I noticed broken semantic versioning on the 1.0.x versions... But the version numbers are consistent enough to get back to correct versioning.
- FIXED: [#56](https://github.com/espresso3389/flutter_pdf_render/issues/56) Null check operator used on a null value

## 1.0.23

- Add PdfViewerController.goToPointInPage/setZoomRatio ([#71](https://github.com/espresso3389/flutter_pdf_render/issues/71)/[#72](https://github.com/espresso3389/flutter_pdf_render/issues/72))
- Document updates

## 1.0.21

- Minor updates.

## 1.0.20

- PdfViewerParams introduces scrollDirection to support horizontal scroll ([#69](https://github.com/espresso3389/flutter_pdf_render/issues/69)).
- targetSdkVersion 31 for example code
- Gradle/Kotlin version updates

## 1.0.19

- Add more error handling codes.

## 1.0.18

- Add workaround for recent Flutter Web breaking change ([#60](https://github.com/espresso3389/flutter_pdf_render/issues/60)).

## 1.0.17

- Additional null check on updateTex (Android); possible fix for [#59](https://github.com/espresso3389/flutter_pdf_render/issues/59) and [#61](https://github.com/espresso3389/flutter_pdf_render/issues/61).

## 1.0.16

- Fix a warning on AndroidManifest.xml.

## 1.0.15

- Merge PR [#55](https://github.com/espresso3389/flutter_pdf_render/issues/55) Add flutter_lints

## 1.0.14

- Remove device_info_plus from dependencies.

## 1.0.13

- Merged PR [#54](https://github.com/espresso3389/flutter_pdf_render/issues/54) "device_info replace by device_info_plus" by lsaudon

## 1.0.12

- Fix [#50](https://github.com/espresso3389/flutter_pdf_render/issues/50) Hot Reload throws error on Change Notifier

## 1.0.11

- Implements page memory purging ([#44](https://github.com/espresso3389/flutter_pdf_render/issues/44))
- Unified code for iOS/macOS
- Better error handling code for iOS/macOS ([#44](https://github.com/espresso3389/flutter_pdf_render/issues/44))

## 1.0.10

- More null-check codes for possible runtime exceptions on dispose ([#42](https://github.com/espresso3389/flutter_pdf_render/issues/42)).

## 1.0.9

- Now supports macOS.

## 1.0.8

- More null-safety updates on PdfDocument.openXXX functions; they were returning null on certain parameter errors but now they throw exception on such errors.

## 1.0.7

- Introducing onError handler on PdfViewer.openXXX and PdfDocumentLoader.openXXX.

## 1.0.6

- Additional fix for [#35](https://github.com/espresso3389/flutter_pdf_render/issues/35) web - renders blank, with no console error

## 1.0.5

- Fix [#35](https://github.com/espresso3389/flutter_pdf_render/issues/35) web - renders blank, with no console error

## 1.0.4

- BREAKING CHANGES: PdfViewer and PdfDocumentLoader now has openFile, openAsset, and openData factory methods. Existing code must be changed to use these methods. Further more, the parameters on PdfViewer is moved to PdfViewerParams.

## 1.0.3

- Update documentation.

## 1.0.2

- cMapUrl setting fix for Flutter Web.

## 1.0.1

- Flutter Web support now works correctly.

## 1.0.0

- Stable release for null-safety.

## 1.0.0-dev

- Initial release that supports null-safety (Currently not compatible with Flutter stable).
- BREAKING CHANGE: pdf_render_widget2 overwrites pdf_render_widget.
- Alpha preview of Flutter Web support (I mean, not yet working correctly).

## 0.69.0

- allowAntialiasingIOS support.

## 0.68.0

- Update device_info version.

## 0.67.0

- PdfViewerController now supports viewRect, zoomRatio, visiblePages, and currentPageNumber.

## 0.66.2

- FIXED: SDK version requirement is not compatible to stable releases.

## 0.66.0

- Introducing PdfViewer that supports interactive pinch-zoom ([#5](https://github.com/espresso3389/flutter_pdf_render/issues/5)); not yet complete but things just work well.

## 0.65.0

- Fixes PDF page rotation issue on iOS ([#18](https://github.com/espresso3389/flutter_pdf_render/issues/18))
- Fixes Y-axis flipping issue on iOS ([#9](https://github.com/espresso3389/flutter_pdf_render/issues/9))

## 0.64.1

- Update documents.
- BREAKING CHANGE: PdfDocument/PdfPage/PdfPageImage no longer have public constructors.

## 0.64.0

- Introducing faster and memory-friendly PdfPageImage rendering mechanism based on Pointer<Uint8> (dart:ffi).
- BREAKING CHANGE: backgroundFill/renderingPixelRatio/dontUseTexture are moved to PdfPageTextureBuilder.
- BREAKING CHANGE: PdfPageImage.image is replaced by PdfPageImage.pixels, PdfPageImage.createImageIfNotAvailable, createImageIfNotAvailable.imageIfAvailable.

## 0.63.0

- Introducing PdfDocumentLoader.onError to handle document open error ([#17](https://github.com/espresso3389/flutter_pdf_render/issues/17) by Sp4Rx).

## 0.62.0

- Minor bug fix.

## 0.61.0

- Introduces pdf_render_widgets2.dart. The classes in pdf_render_widgets.dart are deprecated now.

## 0.57.2

- Update pubspec.yaml not to be shown as WEB compatible on pub.dev ([#11](https://github.com/espresso3389/flutter_pdf_render/issues/11)).

## 0.57.1

- Update comments ([#6](https://github.com/espresso3389/flutter_pdf_render/issues/6)).

## 0.57.0

- On iOS Simulator, the plugin now uses compatibility rendering mode; to test the actual behavior, please use physical devices.

## 0.56.0

- Woops, backgroundFill must be true for default.

## 0.55.0

- Now render like functions treat null and 0 almost identical.

## 0.54.0

- PdfPage.render method does not handle w=0,h=0 case (Changes on 0.51.0 breaks compatibility with older versions).

## 0.53.0

- Introduces PdfPageFit to specify PDF page size fit rule easier.

## 0.51.0

- PdfPage.render method does not handle w=0,h=0 case.

## 0.49.0

- Just update documents. Also introduces `PdfPageImageTexture` class that is used internally to interact with Flutter's Texture class.

## 0.46.0

- `PdfPageView` uses Texture rather than RawImage.

## 0.37.0

- Introducing `PdfDocumentLoader` and `PdfPageView` that eases PDF view.

## 0.33.0

- FIXED: disposing PdfDocument may cause ArrayIndexOutOfBoundsException. (Android)

## 0.29.0

- Minor build configuration changes.

## 0.27.0

- Add backgroundFill option to render method.

## 0.23.0

- First version that supports Android.

## 0.1.0

- First release.
