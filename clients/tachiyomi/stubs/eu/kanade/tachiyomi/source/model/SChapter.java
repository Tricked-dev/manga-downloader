package eu.kanade.tachiyomi.source.model;

public class SChapter {
    public String url = "";
    public String name = "";
    public float chapter_number = -1F;
    public long date_upload = 0L;

    public static SChapter create() {
        return new SChapter();
    }
}
