# List of absolute paths to libraries that should be bundled with the plugin.
# This list could contain prebuilt libraries, or libraries created by an
# external build triggered from this build file.
list(APPEND PDF_RENDER_PLUGIN_LIBRARIES)

# Any new source files that you add to the plugin should be added here.
list(APPEND PDF_RENDER_PLUGIN_SOURCES
  "src/pdf_render_plugin.cpp"
)

# Fully qualified FLUTTER_ROOT directory.
if(CMAKE_SIZEOF_VOID_P EQUAL 8)
  set(FLUTTER_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../flutter/ephemeral/.plugin_symlinks/pdf_render_maintained/flutter")
else()
  set(FLUTTER_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../flutter/ephemeral/.plugin_symlinks/pdf_render_maintained/flutter")
endif()

# This is the name of the plugin specified in the pubspec.yaml
set(PLUGIN_NAME "pdf_render_maintained")

add_library(${PLUGIN_NAME} SHARED
  ${PDF_RENDER_PLUGIN_SOURCES}
)

apply_standard_settings(${PLUGIN_NAME})
set_property(TARGET ${PLUGIN_NAME} PROPERTY CXX_STANDARD 17)

# List of absolute paths to include directories required to compile this plugin.
list(APPEND PDF_RENDER_PLUGIN_INCLUDE_DIRECTORIES
  "${CMAKE_CURRENT_SOURCE_DIR}"
  "${FLUTTER_ROOT}"
)

target_include_directories(${PLUGIN_NAME} PRIVATE
  ${PDF_RENDER_PLUGIN_INCLUDE_DIRECTORIES}
)

# List of libraries that should be linked with the plugin.
target_link_libraries(${PLUGIN_NAME} PRIVATE
  flutter
  flutter_wrapper_plugin
  ${PDF_RENDER_PLUGIN_LIBRARIES}
)

# List of sources that should be compiled for the plugin.
list(APPEND PLUGIN_BUNDLED_LIBRARIES
  ${PDF_RENDER_PLUGIN_LIBRARIES}
)

set(PDF_RENDER_PLUGIN_BUNDLED_LIBRARIES ${PLUGIN_BUNDLED_LIBRARIES} PARENT_SCOPE)
set(PDF_RENDER_PLUGIN_FOUND TRUE PARENT_SCOPE)


