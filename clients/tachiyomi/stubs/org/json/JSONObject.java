package org.json;

public class JSONObject {
    public JSONObject(String source) {}

    public JSONArray optJSONArray(String name) {
        return null;
    }

    public boolean optBoolean(String name, boolean fallback) {
        return fallback;
    }

    public String optString(String name) {
        return "";
    }

    public String optString(String name, String fallback) {
        return fallback;
    }

    public int optInt(String name, int fallback) {
        return fallback;
    }

    public double optDouble(String name, double fallback) {
        return fallback;
    }
}
