package com.artcraft.eguiandroidprobe;

import com.google.androidgamesdk.GameActivity;

import android.os.Bundle;

public class MainActivity extends GameActivity {

    static {
        // 加载 Rust cdylib（对应 AndroidManifest 里的 android.app.lib_name=main）
        System.loadLibrary("main");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
    }
}
