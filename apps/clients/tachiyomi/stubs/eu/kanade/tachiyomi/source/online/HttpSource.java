package eu.kanade.tachiyomi.source.online;

import java.util.List;

import eu.kanade.tachiyomi.source.Source;
import eu.kanade.tachiyomi.source.model.FilterList;
import eu.kanade.tachiyomi.source.model.MangasPage;
import eu.kanade.tachiyomi.source.model.Page;
import eu.kanade.tachiyomi.source.model.SChapter;
import eu.kanade.tachiyomi.source.model.SManga;
import okhttp3.Headers;
import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.Response;
import rx.Observable;

public abstract class HttpSource implements Source {
    public abstract String getBaseUrl();
    public abstract String getLang();
    public abstract boolean getSupportsLatest();
    public abstract Request popularMangaRequest(int page);
    public abstract MangasPage popularMangaParse(Response response) throws Exception;
    public abstract Request latestUpdatesRequest(int page);
    public abstract MangasPage latestUpdatesParse(Response response) throws Exception;
    public abstract Request searchMangaRequest(int page, String query, FilterList filters);
    public abstract MangasPage searchMangaParse(Response response) throws Exception;
    public abstract Request mangaDetailsRequest(SManga manga);
    public abstract SManga mangaDetailsParse(Response response) throws Exception;
    public abstract Request chapterListRequest(SManga manga);
    public abstract List<SChapter> chapterListParse(Response response) throws Exception;
    public abstract Request pageListRequest(SChapter chapter);
    public abstract List<Page> pageListParse(Response response) throws Exception;
    public abstract String imageUrlParse(Response response) throws Exception;

    public long getId() {
        return 0L;
    }

    public int getVersionId() {
        return 1;
    }

    public OkHttpClient getClient() {
        return null;
    }

    public Headers getHeaders() {
        return headersBuilder().build();
    }

    protected Headers.Builder headersBuilder() {
        return new Headers.Builder();
    }

    public Request imageRequest(Page page) {
        return null;
    }

    public FilterList getFilterList() {
        return new FilterList();
    }

    public Observable<MangasPage> fetchSearchManga(int page, String query, FilterList filters) {
        return null;
    }
}
