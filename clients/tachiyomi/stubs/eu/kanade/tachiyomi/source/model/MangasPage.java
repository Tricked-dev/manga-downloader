package eu.kanade.tachiyomi.source.model;

import java.util.List;

public class MangasPage {
    public final List<SManga> mangas;
    public final boolean hasNextPage;

    public MangasPage(List<SManga> mangas, boolean hasNextPage) {
        this.mangas = mangas;
        this.hasNextPage = hasNextPage;
    }
}
