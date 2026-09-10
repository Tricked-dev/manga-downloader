package eu.kanade.tachiyomi.source.model;

public class Page {
    public int index;
    public String url;
    public String imageUrl;

    public Page(int index, String url, String imageUrl) {
        this.index = index;
        this.url = url;
        this.imageUrl = imageUrl;
    }
}
