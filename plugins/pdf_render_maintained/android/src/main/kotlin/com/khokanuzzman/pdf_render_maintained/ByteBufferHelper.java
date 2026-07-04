package com.khokanuzzman.pdf_render_maintained;

import java.nio.ByteBuffer;

class ByteBufferHelper {
  static {
    System.loadLibrary("bbhelper");
  }

  public static native ByteBuffer newDirectBuffer(long ptr, long size);
  public static native long malloc(long size);
  public static native void free(long ptr);
}
