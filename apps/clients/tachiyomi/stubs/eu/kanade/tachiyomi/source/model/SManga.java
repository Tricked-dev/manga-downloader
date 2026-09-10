package eu.kanade.tachiyomi.source.model;

public class SManga {
    public static final int UNKNOWN = 0;
    public static final int ONGOING = 1;
    public static final int COMPLETED = 2;

    public String url = "";
    public String title = "";
    public String author = "";
    public String description = "";
    public String genre = "";
    public String thumbnail_url = "";
    public int status = UNKNOWN;

    public static SManga create() {
        return new SManga();
    }
}
