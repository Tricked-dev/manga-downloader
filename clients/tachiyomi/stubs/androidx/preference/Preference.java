package androidx.preference;

public class Preference {
    public interface OnPreferenceChangeListener {
        boolean onPreferenceChange(Preference preference, Object newValue);
    }

    public void setKey(String key) {}
    public void setTitle(CharSequence title) {}
    public void setSummary(CharSequence summary) {}
    public void setDefaultValue(Object defaultValue) {}
    public void setOnPreferenceChangeListener(OnPreferenceChangeListener listener) {}
}
