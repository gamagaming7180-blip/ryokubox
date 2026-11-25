package com.ryoku.ryokubox;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
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

/** Ryoku Box — fullscreen landscape companion for the Lithic deck on the PC. */
public class MainActivity extends Activity {

    private static final String SERVICE_TYPE = "_ryokubox._tcp";
    private static final String DEFAULT_HOST = BuildConfig.FALLBACK_HOST;
    private static final String SELF_PAIR_KEY = BuildConfig.TOKEN;

    private WebView web;
    private View connectPanel;
    private EditText hostField;
    private EditText codeField;
    private SharedPreferences prefs;
    private NsdManager nsd;
    private NsdManager.DiscoveryListener discoveryListener;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private boolean connected = false;

    private final Runnable fallback = new Runnable() {
        @Override public void run() {
            if (connected) return;
            connected = true;
            stopDiscovery();
            // Always use the PC's LAN address baked at build time; stale 127.0.0.1
            // entries from the old adb-reverse setup never apply here.
            load(DEFAULT_HOST, SELF_PAIR_KEY);
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
        // This is an always-on desk deck: never sleep while it is in front.
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
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, String url) {
                return false;
            }
            @Override
            public void onReceivedError(WebView view, int errorCode, String description, String failingUrl) {
                showConnect("Cannot reach the agent. Check the network, then retry.");
            }
        });
        root.addView(web, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        // Manual Link & Pairing stays as a hidden fallback only — normal path needs no input.
        buildConnectPanel();
        root.addView(connectPanel, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        setContentView(root);

        hostField.setText(prefs.getString("host", DEFAULT_HOST));
        connectPanel.setVisibility(View.GONE);
        autoConnect();
    }

    /** Find the PC over mDNS and open the deck. No host, code, ADB or firewall steps. */
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
                                // The PC publishes its IPv4 in the mDNS TXT record so an
                                // IPv6-only resolve still yields a working address.
                                String ip = null;
                                try {
                                    byte[] raw = i.getAttributes().get("ip");
                                    if (raw != null) ip = new String(raw, "UTF-8").trim();
                                } catch (Exception ignored) { }
                                String host = (ip == null || ip.isEmpty())
                                        ? DEFAULT_HOST : ip + ":" + i.getPort();
                                handler.removeCallbacks(fallback);
                                stopDiscovery();
                                if (connected) return;
                                connected = true;
                                prefs.edit().putString("host", host).putBoolean("paired", true).apply();
                                load(host, SELF_PAIR_KEY);
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

    private void stopDiscovery() {
        try {
            if (nsd != null && discoveryListener != null) nsd.stopServiceDiscovery(discoveryListener);
        } catch (Exception ignored) { }
    }

    private void buildConnectPanel() {
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setGravity(Gravity.CENTER);
        panel.setBackgroundColor(Color.argb(235, 0, 0, 0));
        panel.setPadding(48, 32, 48, 32);

        TextView mark = new TextView(this);
        mark.setText("力");
        mark.setTextColor(Color.rgb(206, 94, 50));
        mark.setTextSize(46);
        mark.setGravity(Gravity.CENTER);
        panel.addView(mark);

        TextView title = new TextView(this);
        title.setText("RYOKU BOX");
        title.setTextColor(Color.rgb(205, 196, 186));
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setTextSize(22);
        title.setGravity(Gravity.CENTER);
        panel.addView(title);

        TextView hint = new TextView(this);
        hint.setText("Connect to the agent on your computer");
        hint.setTextColor(Color.rgb(149, 143, 135));
        hint.setTextSize(13);
        hint.setGravity(Gravity.CENTER);
        hint.setPadding(0, 8, 0, 28);
        panel.addView(hint);

        hostField = field("Host  (auto-discovered normally)");
        panel.addView(hostField);

        codeField = field("Pairing code (only if you rotated the token)");
        panel.addView(codeField);

        Button connect = new Button(this);
        connect.setText("CONNECT");
        connect.setTextColor(Color.BLACK);
        connect.setBackgroundColor(Color.rgb(206, 94, 50));
        LinearLayout.LayoutParams bp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        bp.topMargin = 18;
        connect.setLayoutParams(bp);
        connect.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String host = hostField.getText().toString().trim();
                String code = codeField.getText().toString().trim();
                if (host.isEmpty()) host = DEFAULT_HOST;
                if (code.isEmpty()) code = SELF_PAIR_KEY;
                connected = true;
                handler.removeCallbacks(fallback);
                stopDiscovery();
                prefs.edit().putString("host", host).putBoolean("paired", true).apply();
                load(host, code);
            }
        });
        panel.addView(connect);

        TextView foot = new TextView(this);
        foot.setText("Auto Link & Pair over Wi-Fi — no cable, no debugging");
        foot.setTextColor(Color.rgb(149, 143, 135));
        foot.setTextSize(12);
        foot.setGravity(Gravity.CENTER);
        foot.setPadding(0, 24, 0, 0);
        panel.addView(foot);

        connectPanel = panel;
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
        p.bottomMargin = 12;
        e.setLayoutParams(p);
        return e;
    }

    private void showConnect(String message) {
        handler.removeCallbacks(fallback);
        stopDiscovery();
        connectPanel.setVisibility(View.VISIBLE);
        if (message != null) Toast.makeText(this, message, Toast.LENGTH_LONG).show();
    }

    private void load(String host, String code) {
        String base = host.contains("://") ? host : "http://" + host;
        if (!base.endsWith("/")) base += "/";
        String url = base;
        if (code != null && !code.isEmpty()) {
            url = base + "?pair=" + Uri.encode(code);
            codeField.setText("");
        }
        connectPanel.setVisibility(View.GONE);
        web.loadUrl(url);
    }

    @Override
    public void onBackPressed() {
        if (connectPanel.getVisibility() == View.VISIBLE) {
            connectPanel.setVisibility(View.GONE);
        } else if (web.canGoBack()) {
            web.goBack();
        } else {
            super.onBackPressed();
        }
    }

    /* Physical volume keys drive the computer's sink, not the phone's. */
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
                View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY);
    }
}
