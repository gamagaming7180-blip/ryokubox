package com.ryoku.ryokubox;

import android.content.Context;
import android.graphics.ImageFormat;
import android.hardware.Camera;
import android.util.Log;
import android.view.SurfaceHolder;
import android.view.SurfaceView;

import com.google.zxing.BinaryBitmap;
import com.google.zxing.MultiFormatReader;
import com.google.zxing.PlanarYUVLuminanceSource;
import com.google.zxing.common.HybridBinarizer;

/** Minimal in-app QR reader: camera preview + ZXing decode, no camera app. */
class QrScanner implements SurfaceHolder.Callback, Camera.PreviewCallback {

    interface Listener { void onResult(String text); }

    private final SurfaceView view;
    private final Listener listener;
    private Camera camera;
    private volatile boolean running;

    QrScanner(SurfaceView view, Listener listener) {
        this.view = view;
        this.listener = listener;
        view.getHolder().addCallback(this);
    }

    void start() {
        running = true;
        open();
    }

    void stop() {
        running = false;
        if (camera != null) {
            try { camera.setPreviewCallback(null); camera.stopPreview(); camera.release(); }
            catch (Exception ignored) { }
            camera = null;
        }
    }

    private void open() {
        if (!running) return;
        try {
            camera = Camera.open();
            camera.setDisplayOrientation(0);
            Camera.Parameters p = camera.getParameters();
            p.setPreviewFormat(ImageFormat.NV21);
            Camera.Size best = null;
            for (Camera.Size s : p.getSupportedPreviewSizes()) {
                if (best == null || (long) s.width * s.height > (long) best.width * best.height) best = s;
            }
            if (best != null) p.setPreviewSize(best.width, best.height);
            camera.setParameters(p);
            camera.setPreviewCallback(this);
            camera.startPreview();
        } catch (Exception e) {
            Log.w("RyokuBox", "camera open failed", e);
        }
    }

    @Override public void surfaceCreated(SurfaceHolder holder) { if (running) open(); }
    @Override public void surfaceChanged(SurfaceHolder holder, int f, int w, int h) { }
    @Override public void surfaceDestroyed(SurfaceHolder holder) { stop(); }

    @Override
    public void onPreviewFrame(byte[] data, Camera cam) {
        if (!running || data == null) return;
        try {
            Camera.Size size = cam.getParameters().getPreviewSize();
            int w = size.width, h = size.height;
            PlanarYUVLuminanceSource src = new PlanarYUVLuminanceSource(data, w, h, 0, 0, w, h, false);
            BinaryBitmap bitmap = new BinaryBitmap(new HybridBinarizer(src));
            String text = new MultiFormatReader().decode(bitmap).getText();
            if (text != null && !text.isEmpty()) {
                running = false;
                listener.onResult(text);
            }
        } catch (Exception ignored) { /* no QR in this frame */ }
    }
}
