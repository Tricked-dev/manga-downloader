package android.content;

public class Context {
    public static final int MODE_PRIVATE = 0;

    public Context getApplicationContext() {
        return this;
    }

    public SharedPreferences getSharedPreferences(String name, int mode) {
        return null;
    }
}
