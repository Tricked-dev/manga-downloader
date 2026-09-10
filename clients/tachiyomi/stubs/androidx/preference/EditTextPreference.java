package androidx.preference;

import android.content.Context;
import android.widget.EditText;

public class EditTextPreference extends Preference {
    public interface OnBindEditTextListener {
        void onBindEditText(EditText editText);
    }

    public EditTextPreference(Context context) {}

    public void setOnBindEditTextListener(OnBindEditTextListener listener) {}
}
