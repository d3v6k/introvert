#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint pdf_render_maintained.podspec' to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'pdf_render_maintained'
  s.version          = '1.6.1'
  s.summary          = 'PDF rendering plugin for Flutter (maintained fork).'
  s.description      = <<-DESC
Bindings that expose fast PDF page rendering APIs and widgets for Flutter.
  DESC
  s.homepage         = 'https://github.com/khokanuzzman/pdf_render_maintained'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'pdf_render_maintained authors' => 'maintainers@pdf-render-maintained.local' }
  s.source           = { :path => '.' }
  s.source_files     = 'Classes/**/*'
  s.dependency       'Flutter'
  # Align with Flutter's current minimum iOS target.
  s.platform         = :ios, '13.0'

  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES' }
  s.swift_version = '5.0'
end
