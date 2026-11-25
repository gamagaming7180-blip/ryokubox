package com.ryoku.ryokubox;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.net.Uri;
import android.net.nsd.NsdManager;
import android.net.nsd.NsdServiceInfo;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowManager;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;

/** Ryoku Box — fullscreen companion for the Lithic deck on the PC. No baked secrets. */
public class MainActivity extends Activity {

    private static final String SERVICE_TYPE = "_ryokubox._tcp";
    private static final String DEFAULT_HOST = BuildConfig.FALLBACK_HOST;
    private static final int CAMERA_REQ = 7;

    private WebView web;
    private View connectPanel;
    private View scannerPanel;
    private EditText hostField;
    private EditText codeField;
    private EditText tokenField;
    private SharedPreferences prefs;
    private NsdManager nsd;
    private NsdManager.DiscoveryListener discoveryListener;
    private QrScanner scanner;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private boolean connected = false;
    private String pendingToken = null;

    private final Runnable fallback = new Runnable() {
        @Override public void run() {
            if (connected) return;
            connected = true;
            stopDiscovery();
            showConnect(null);
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        setTheme(android.R.style.Theme_Material_NoActionBar);
        super.onCreate(savedInstanceState);

        prefs = getSharedPreferences("ryokubox", Context.MODE_PRIVATE);
        getWindow().setStatusBarColor(Color.BLACK);
        getWindow().setNavigationBarColor(Color.BLACK);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN
                | WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);

        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.BLACK);

        web = new WebView(this);
        web.setBackgroundColor(Color.BLACK);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setDatabaseEnabled(true);
        s.setLoadWithOverviewMode(true);
        s.setUseWideViewPort(true);
        s.setCacheMode(WebSettings.LOAD_NO_CACHE);
        s.setMediaPlaybackRequiresUserGesture(false);
        s.setTextZoom(100);
        web.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, String url) { return false; }
            @Override public void onReceivedError(WebView view, int errorCode, String description, String failingUrl) {
                showConnect("Cannot reach the agent. Check the network, then retry.");
            }
            @Override public void onPageFinished(WebView view, String url) {
                if (pendingToken != null) {
                    final String t = pendingToken;
                    pendingToken = null;
                    view.evaluateJavascript(
                            "localStorage.setItem('ryokubox.token','" + t + "');location.reload();", null);
                }
            }
        });
        root.addView(web, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        buildConnectPanel();
        root.addView(connectPanel, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        buildScannerPanel();
        root.addView(scannerPanel, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        setContentView(root);

        hostField.setText(prefs.getString("host", DEFAULT_HOST));
        connectPanel.setVisibility(View.GONE);
        scannerPanel.setVisibility(View.GONE);
        if (prefs.getBoolean("paired", false)) {
            connected = true;
            load(prefs.getString("host", DEFAULT_HOST), null, null);
        } else {
            connected = false;
            handler.postDelayed(fallback, 5000);
            // The baked address is only an IP, not a secret. Ask the agent for a
            // fresh one-time code directly — no camera and no mDNS required.
            fetchPairCode(DEFAULT_HOST);
        }
    }

    /** Built-in scanner: reads the mDNS TXT record, not the camera. */
    private void autoConnect() {
        connected = false;
        handler.postDelayed(fallback, 4000);
        try {
            nsd = (NsdManager) getSystemService(Context.NSD_SERVICE);
            discoveryListener = new NsdManager.DiscoveryListener() {
                @Override public void onDiscoveryStarted(String type) { }
                @Override public void onStartDiscoveryFailed(String type, int errorCode) { fallback.run(); }
                @Override public void onServiceLost(NsdServiceInfo info) { }
                @Override public void onStopDiscoveryFailed(String type, int errorCode) { }
                @Override public void onDiscoveryStopped(String type) { }

                @Override public void onServiceFound(NsdServiceInfo info) {
                    if (info == null || !SERVICE_TYPE.equals(info.getServiceType())) return;
                    try {
                        nsd.resolveService(info, new NsdManager.ResolveListener() {
                            @Override public void onResolveFailed(NsdServiceInfo i, int code) { }
                            @Override public void onServiceResolved(NsdServiceInfo i) {
                                String ip = txt(i, "ip");
                                String host = (ip == null || ip.isEmpty())
                                        ? DEFAULT_HOST : ip + ":" + i.getPort();
                                handler.removeCallbacks(fallback);
                                stopDiscovery();
                                if (connected) return;
                                fetchPairCode(host);
                            }
                        });
                    } catch (Exception ignored) { }
                }
            };
            nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discoveryListener);
        } catch (Exception e) {
            fallback.run();
        }
    }

    /** Ask the discovered agent for a fresh one-time code — no camera, no baked token. */
    private void fetchPairCode(final String host) {
        new Thread(new Runnable() {
            @Override public void run() {
                String code = null;
                HttpURLConnection conn = null;
                try {
                    conn = (HttpURLConnection) new URL("http://" + host + "/api/pair-code").openConnection();
                    conn.setConnectTimeout(2500);
                    conn.setReadTimeout(2500);
                    BufferedReader reader = new BufferedReader(new InputStreamReader(conn.getInputStream()));
                    String body = reader.readLine();
                    reader.close();
                    if (body != null) {
                        int k = body.indexOf("\"code\"");
                        if (k >= 0) {
                            int colon = body.indexOf(':', k);
                            int a = body.indexOf('"', colon + 1);
                            int b = body.indexOf('"', a + 1);
                            if (a > 0 && b > a) code = body.substring(a + 1, b);
                        }
                    }
                } catch (Exception ignored) { }
                finally { if (conn != null) conn.disconnect(); }

                final String result = code;
                handler.post(new Runnable() {
                    @Override public void run() {
                        if (connected) return;
                        if (result == null || result.isEmpty()) { showConnect(null); return; }
                        connected = true;
                        handler.removeCallbacks(fallback);
                        save(host, true);
                        load(host, result, null);
                    }
                });
            }
        }).start();
    }

    private String txt(NsdServiceInfo info, String key) {
        try {
            byte[] raw = info.getAttributes().get(key);
            return raw == null ? null : new String(raw, "UTF-8").trim();
        } catch (Exception e) { return null; }
    }

    private void stopDiscovery() {
        try { if (nsd != null && discoveryListener != null) nsd.stopServiceDiscovery(discoveryListener); }
        catch (Exception ignored) { }
    }

    private void buildConnectPanel() {
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setGravity(Gravity.CENTER);
        panel.setBackgroundColor(Color.argb(235, 0, 0, 0));
        panel.setPadding(40, 24, 40, 24);

        TextView mark = new TextView(this);
        mark.setText("力"); mark.setTextColor(Color.rgb(206, 94, 50));
        mark.setTextSize(38); mark.setGravity(Gravity.CENTER);
        panel.addView(mark);

        TextView title = new TextView(this);
        title.setText("RYOKU BOX"); title.setTextColor(Color.rgb(205, 196, 186));
        title.setTypeface(Typeface.DEFAULT_BOLD); title.setTextSize(20); title.setGravity(Gravity.CENTER);
        panel.addView(title);

        TextView hint = new TextView(this);
        hint.setText("Auto-links over Wi-Fi. Or scan / paste a code below.");
        hint.setTextColor(Color.rgb(149, 143, 135)); hint.setTextSize(12);
        hint.setGravity(Gravity.CENTER); hint.setPadding(0, 6, 0, 16);
        panel.addView(hint);

        hostField = field("Host (auto)");
        panel.addView(hostField);
        codeField = field("One-time code from console QR");
        panel.addView(codeField);
        tokenField = field("Or paste permanent token from console");
        panel.addView(tokenField);

        Button scan = button("SCAN QR (camera)", Color.rgb(206, 94, 50));
        scan.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { startScanner(); }
        });
        panel.addView(scan);

        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        Button pair = button("PAIR", Color.rgb(70, 70, 70));
        pair.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String host = host(hostField);
                String code = codeField.getText().toString().trim();
                if (code.isEmpty()) { toast("Enter the one-time code, or scan the QR."); return; }
                connected = true; handler.removeCallbacks(fallback); stopDiscovery();
                save(host, true); load(host, code, null);
            }
        });
        Button useToken = button("USE TOKEN", Color.rgb(70, 70, 70));
        useToken.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String host = host(hostField);
                String token = tokenField.getText().toString().trim();
                if (token.isEmpty()) { toast("Paste the token from the PC console."); return; }
                connected = true; handler.removeCallbacks(fallback); stopDiscovery();
                save(host, true); load(host, null, token);
            }
        });
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0,
                ViewGroup.LayoutParams.WRAP_CONTENT, 1);
        lp.rightMargin = 8;
        pair.setLayoutParams(lp);
        useToken.setLayoutParams(new LinearLayout.LayoutParams(0,
                ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        row.addView(pair); row.addView(useToken);
        panel.addView(row);

        connectPanel = panel;
    }

    private void buildScannerPanel() {
        FrameLayout panel = new FrameLayout(this);
        panel.setBackgroundColor(Color.BLACK);
        SurfaceView surface = new SurfaceView(this);
        panel.addView(surface, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        TextView hint = new TextView(this);
        hint.setText("Point at the console QR");
        hint.setTextColor(Color.WHITE);
        hint.setBackgroundColor(Color.argb(150, 0, 0, 0));
        hint.setGravity(Gravity.CENTER);
        hint.setPadding(12, 12, 12, 12);
        FrameLayout.LayoutParams hp = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.TOP | Gravity.CENTER_HORIZONTAL);
        hp.topMargin = 24;
        hint.setLayoutParams(hp);
        panel.addView(hint);
        Button cancel = new Button(this);
        cancel.setText("CANCEL");
        cancel.setBackgroundColor(Color.rgb(206, 94, 50));
        cancel.setTextColor(Color.BLACK);
        FrameLayout.LayoutParams cp = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.BOTTOM | Gravity.CENTER_HORIZONTAL);
        cp.bottomMargin = 32;
        cancel.setLayoutParams(cp);
        panel.addView(cancel);
        scanner = new QrScanner(surface, new QrScanner.Listener() {
            @Override public void onResult(String text) { onQr(text); }
        });
        cancel.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { closeScanner(); }
        });
        scannerPanel = panel;
    }

    private void startScanner() {
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            openScanner();
        } else {
            requestPermissions(new String[]{Manifest.permission.CAMERA}, CAMERA_REQ);
        }
    }

    private void openScanner() {
        connectPanel.setVisibility(View.GONE);
        scannerPanel.setVisibility(View.VISIBLE);
        scanner.start();
    }

    private void closeScanner() {
        scanner.stop();
        scannerPanel.setVisibility(View.GONE);
        connectPanel.setVisibility(View.VISIBLE);
    }

    @Override
    public void onRequestPermissionsResult(int req, String[] perms, int[] grants) {
        if (req == CAMERA_REQ && grants.length > 0 && grants[0] == PackageManager.PERMISSION_GRANTED) {
            openScanner();
        } else {
            toast("Camera permission needed for the QR scanner.");
        }
    }

    private void onQr(String text) {
        try {
            Uri u = Uri.parse(text);
            String code = u.getQueryParameter("pair");
            String host = u.getHost() + ":" + u.getPort();
            if (code == null || code.isEmpty() || host.endsWith(":-1")) throw new IllegalArgumentException();
            closeScanner();
            connected = true; handler.removeCallbacks(fallback); stopDiscovery();
            save(host, true); load(host, code, null);
        } catch (Exception e) {
            toast("That QR is not a Ryoku Box pairing code.");
            closeScanner();
        }
    }

    private String host(EditText field) {
        String h = field.getText().toString().trim();
        return h.isEmpty() ? DEFAULT_HOST : h;
    }

    private void save(String host, boolean paired) {
        prefs.edit().putString("host", host).putBoolean("paired", paired).apply();
    }

    private EditText field(String hintText) {
        EditText e = new EditText(this);
        e.setHint(hintText);
        e.setHintTextColor(Color.rgb(120, 114, 107));
        e.setTextColor(Color.rgb(205, 196, 186));
        e.setBackgroundColor(Color.rgb(35, 33, 31));
        e.setSingleLine(true);
        e.setPadding(20, 14, 20, 14);
        LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        p.bottomMargin = 10;
        e.setLayoutParams(p);
        return e;
    }

    private Button button(String label, int bg) {
        Button b = new Button(this);
        b.setText(label);
        b.setTextColor(Color.BLACK);
        b.setBackgroundColor(bg);
        return b;
    }

    private void toast(String msg) { Toast.makeText(this, msg, Toast.LENGTH_LONG).show(); }

    private void showConnect(String message) {
        handler.removeCallbacks(fallback);
        stopDiscovery();
        connectPanel.setVisibility(View.VISIBLE);
        if (message != null) toast(message);
    }

    private void load(String host, String code, String token) {
        String base = host.contains("://") ? host : "http://" + host;
        if (!base.endsWith("/")) base += "/";
        String url = base;
        if (code != null && !code.isEmpty()) url = base + "?pair=" + Uri.encode(code);
        else if (token != null && !token.isEmpty()) { pendingToken = token; }
        connectPanel.setVisibility(View.GONE);
        scannerPanel.setVisibility(View.GONE);
        web.loadUrl(url);
    }

    @Override
    public void onBackPressed() {
        if (scannerPanel.getVisibility() == View.VISIBLE) { closeScanner(); return; }
        if (connectPanel.getVisibility() == View.VISIBLE) { connectPanel.setVisibility(View.GONE); return; }
        if (web.canGoBack()) web.goBack(); else super.onBackPressed();
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        int key = event.getKeyCode();
        if (key == KeyEvent.KEYCODE_VOLUME_UP || key == KeyEvent.KEYCODE_VOLUME_DOWN) {
            if (event.getAction() == KeyEvent.ACTION_DOWN) {
                int delta = key == KeyEvent.KEYCODE_VOLUME_UP ? 5 : -5;
                web.evaluateJavascript("window.rbAdjustVolume && window.rbAdjustVolume(" + delta + ")", null);
            }
            return true;
        }
        return super.dispatchKeyEvent(event);
    }

    @Override
    protected void onResume() {
        super.onResume();
        getWindow().getDecorView().setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY);
    }
}
