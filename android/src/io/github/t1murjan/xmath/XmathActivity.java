package io.github.t1murjan.xmath;

import android.app.NativeActivity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.print.PageRange;
import android.print.PrintAttributes;
import android.print.PrintDocumentAdapter;
import android.print.PrintDocumentInfo;
import android.print.PrintManager;
import android.provider.OpenableColumns;
import android.text.InputType;
import android.util.AtomicFile;
import android.view.KeyEvent;
import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;
import android.widget.Toast;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.Locale;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicReference;
import java.nio.charset.StandardCharsets;

public final class XmathActivity extends NativeActivity {
    static {
        System.loadLibrary("xmath_gui");
    }

    private static final int REQUEST_BASE = 100;
    private static final int OPEN = 0;
    private static final int SAVE = 1;
    private static final int PDF = 2;
    private static final int REFERENCE = 3;

    private final AtomicReference<String[]> documentResult = new AtomicReference<>();
    private final ConcurrentHashMap<String, Uri> destinations = new ConcurrentHashMap<>();
    private final ArrayList<String> input = new ArrayList<>();
    private KeyboardView keyboard;

    private static native void wake();

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        keyboard = new KeyboardView();
        addContentView(keyboard, new FrameLayout.LayoutParams(1, 1));
    }

    @Override
    public void onGlobalLayout() {
        super.onGlobalLayout();
        wake();
    }

    public void setKeyboard(boolean visible, boolean restart) {
        runOnUiThread(() -> {
            InputMethodManager ime = getSystemService(InputMethodManager.class);
            if (visible) {
                keyboard.requestFocus();
                if (restart) {
                    ime.restartInput(keyboard);
                }
                keyboard.post(() -> {
                    if (keyboard.hasFocus()) {
                        ime.showSoftInput(keyboard, InputMethodManager.SHOW_IMPLICIT);
                    }
                });
            } else {
                ime.hideSoftInputFromWindow(keyboard.getWindowToken(), 0);
                keyboard.clearFocus();
            }
        });
    }

    private void sendInput(String event) {
        synchronized (input) {
            input.add(event);
        }
        wake();
    }

    public String[] takeInput() {
        synchronized (input) {
            if (input.isEmpty()) {
                return null;
            }
            String[] events = input.toArray(new String[0]);
            input.clear();
            return events;
        }
    }

    private final class KeyboardView extends View {
        KeyboardView() {
            super(XmathActivity.this);
            setFocusableInTouchMode(true);
            setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
        }

        @Override
        public boolean onCheckIsTextEditor() {
            return true;
        }

        @Override
        public InputConnection onCreateInputConnection(EditorInfo info) {
            info.inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD;
            info.imeOptions = EditorInfo.IME_ACTION_DONE | EditorInfo.IME_FLAG_NO_EXTRACT_UI;
            info.initialSelStart = 0;
            info.initialSelEnd = 0;
            return new BaseInputConnection(this, true) {
                private String composing;

                @Override
                public boolean setComposingText(CharSequence text, int cursor) {
                    if (composing == null) {
                        sendInput("ime-start");
                    }
                    composing = text.toString();
                    sendInput("preedit:" + composing);
                    return true;
                }

                @Override
                public boolean finishComposingText() {
                    if (composing != null) {
                        sendInput("commit:" + composing);
                        composing = null;
                    }
                    return true;
                }

                @Override
                public boolean commitText(CharSequence text, int cursor) {
                    if (composing != null) {
                        sendInput("commit:" + text);
                        composing = null;
                    } else if ("\n".contentEquals(text)) {
                        sendInput("enter");
                    } else {
                        sendInput("text:" + text);
                    }
                    return true;
                }

                @Override
                public boolean deleteSurroundingText(int before, int after) {
                    finishComposingText();
                    for (int i = 0; i < before; i++) {
                        sendInput("backspace");
                    }
                    for (int i = 0; i < after; i++) {
                        sendInput("delete");
                    }
                    return true;
                }

                @Override
                public boolean deleteSurroundingTextInCodePoints(int before, int after) {
                    return deleteSurroundingText(before, after);
                }

                @Override
                public boolean performEditorAction(int action) {
                    finishComposingText();
                    sendInput("enter");
                    return true;
                }

                @Override
                public boolean sendKeyEvent(KeyEvent event) {
                    if (event.getAction() != KeyEvent.ACTION_DOWN) {
                        return true;
                    }
                    finishComposingText();
                    switch (event.getKeyCode()) {
                        case KeyEvent.KEYCODE_DEL: sendInput("backspace"); break;
                        case KeyEvent.KEYCODE_FORWARD_DEL: sendInput("delete"); break;
                        case KeyEvent.KEYCODE_ENTER: sendInput("enter"); break;
                        case KeyEvent.KEYCODE_DPAD_LEFT: sendInput("left"); break;
                        case KeyEvent.KEYCODE_DPAD_RIGHT: sendInput("right"); break;
                        default:
                            int codepoint = event.getUnicodeChar();
                            if (Character.isValidCodePoint(codepoint) && codepoint > 0) {
                                sendInput("text:" + new String(Character.toChars(codepoint)));
                            }
                    }
                    return true;
                }
            };
        }
    }

    public void pickDocument(int kind, String name) {
        runOnUiThread(() -> {
            Intent intent = new Intent(kind == SAVE || kind == PDF
                    ? Intent.ACTION_CREATE_DOCUMENT : Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            if (kind == OPEN || kind == REFERENCE) {
                intent.setType("*/*");
            } else {
                intent.setType(kind == PDF ? "application/pdf" : "application/octet-stream");
                intent.putExtra(Intent.EXTRA_TITLE, name);
            }
            try {
                startActivityForResult(intent, REQUEST_BASE + kind);
            } catch (RuntimeException error) {
                deliver("error", error.toString());
            }
        });
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        int kind = request - REQUEST_BASE;
        if (kind < OPEN || kind > REFERENCE) {
            return;
        }
        if (result != RESULT_OK || data == null || data.getData() == null) {
            deliver("cancel", "");
            return;
        }
        Uri uri = data.getData();
        new Thread(() -> resolveDocument(kind, uri), "xmath-document").start();
    }

    private void resolveDocument(int kind, Uri uri) {
        try {
            String displayName = null;
            try (Cursor cursor = getContentResolver().query(uri,
                    new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
                if (cursor != null && cursor.moveToFirst()) {
                    displayName = cursor.getString(0);
                }
            }
            if (displayName == null || displayName.isEmpty()) {
                displayName = kind == PDF ? "xmath.pdf" : "xmath.xmcd";
            }
            displayName = displayName.replace('/', '_').replace('\\', '_').replace('\0', '_');
            if (displayName.equals(".") || displayName.equals("..")) {
                displayName = "xmath.xmcd";
            }
            String lowerName = displayName.toLowerCase(Locale.ROOT);
            if (kind == SAVE && !lowerName.endsWith(".xmcd") && !lowerName.endsWith(".xmath")) {
                displayName += ".xmcd";
            } else if (kind == PDF && !lowerName.endsWith(".pdf")) {
                displayName += ".pdf";
            }
            String id = UUID.nameUUIDFromBytes(uri.toString().getBytes(StandardCharsets.UTF_8)).toString();
            File folder = new File(getFilesDir(), "documents/" + id);
            if (!folder.isDirectory() && !folder.mkdirs()) {
                throw new IOException("Не удалось создать рабочую папку");
            }
            File path = new File(folder, displayName);
            if (kind == OPEN || kind == REFERENCE) {
                try (InputStream stream = getContentResolver().openInputStream(uri)) {
                    if (stream == null) {
                        throw new IOException("Не удалось прочитать выбранный файл");
                    }
                    writeLocal(path, stream);
                }
            }
            destinations.put(path.getAbsolutePath(), uri);
            deliver("ok", path.getAbsolutePath());
        } catch (Exception error) {
            deliver("error", error.toString());
        }
    }

    private static void writeLocal(File path, InputStream input) throws IOException {
        AtomicFile file = new AtomicFile(path);
        FileOutputStream output = file.startWrite();
        try {
            copy(input, output);
            file.finishWrite(output);
        } catch (IOException error) {
            file.failWrite(output);
            throw error;
        }
    }

    private static void copy(InputStream input, OutputStream output) throws IOException {
        byte[] buffer = new byte[32 * 1024];
        int size;
        while ((size = input.read(buffer)) != -1) {
            output.write(buffer, 0, size);
        }
    }

    private void deliver(String status, String value) {
        documentResult.set(new String[]{status, value});
        wake();
    }

    public String[] takeDocumentResult() {
        return documentResult.getAndSet(null);
    }

    public String writeDocument(String path, byte[] data) {
        Uri uri = destinations.get(path);
        if (uri == null) {
            return "Файл недоступен. Выберите папку через «Сохранить как…».";
        }
        try (OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
            if (output == null) {
                return "Не удалось открыть файл для записи";
            }
            output.write(data);
        } catch (Exception error) {
            return error.toString();
        }
        try {
            if (path.toLowerCase(Locale.ROOT).endsWith(".pdf")) {
                destinations.remove(path);
            } else {
                writeLocal(new File(path), new java.io.ByteArrayInputStream(data));
            }
        } catch (IOException error) {
            // The selected document has already been written successfully.
            android.util.Log.w("xmath", "Unable to update private worksheet copy", error);
        }
        return null;
    }

    public void printPdf(String title, byte[] data) {
        runOnUiThread(() -> {
            try {
                PrintManager manager = getSystemService(PrintManager.class);
                manager.print(title, new PrintDocumentAdapter() {
                    @Override
                    public void onLayout(PrintAttributes oldAttributes, PrintAttributes newAttributes,
                            CancellationSignal cancellation, LayoutResultCallback callback, Bundle extras) {
                        if (cancellation.isCanceled()) {
                            callback.onLayoutCancelled();
                            return;
                        }
                        PrintDocumentInfo info = new PrintDocumentInfo.Builder(title + ".pdf")
                                .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT).build();
                        callback.onLayoutFinished(info, !newAttributes.equals(oldAttributes));
                    }

                    @Override
                    public void onWrite(PageRange[] pages, ParcelFileDescriptor destination,
                            CancellationSignal cancellation, WriteResultCallback callback) {
                        new Thread(() -> {
                            if (cancellation.isCanceled()) {
                                callback.onWriteCancelled();
                                return;
                            }
                            try (OutputStream stream = new ParcelFileDescriptor.AutoCloseOutputStream(destination)) {
                                stream.write(data);
                            } catch (IOException error) {
                                callback.onWriteFailed(error.getMessage());
                                return;
                            }
                            callback.onWriteFinished(new PageRange[]{PageRange.ALL_PAGES});
                        }, "xmath-print").start();
                    }
                }, null);
            } catch (RuntimeException error) {
                Toast.makeText(this, error.toString(), Toast.LENGTH_LONG).show();
            }
        });
    }
}
