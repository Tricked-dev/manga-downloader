package eu.kanade.tachiyomi.extension.all.mangadownloader

import android.content.Context
import android.content.SharedPreferences
import android.text.InputType
import android.widget.Toast
import androidx.preference.EditTextPreference
import androidx.preference.Preference
import androidx.preference.PreferenceScreen
import eu.kanade.tachiyomi.source.ConfigurableSource
import eu.kanade.tachiyomi.source.UnmeteredSource
import eu.kanade.tachiyomi.source.model.FilterList
import eu.kanade.tachiyomi.source.model.MangasPage
import eu.kanade.tachiyomi.source.model.Page
import eu.kanade.tachiyomi.source.model.SChapter
import eu.kanade.tachiyomi.source.model.SManga
import eu.kanade.tachiyomi.source.online.HttpSource
import java.net.URLEncoder
import java.text.SimpleDateFormat
import java.util.Collections
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.Callable
import okhttp3.Headers
import okhttp3.Request
import okhttp3.Response
import org.json.JSONArray
import org.json.JSONObject
import rx.Observable

private const val DOWNLOADED_CHAPTER_PAGE_ROUTE_PREFIX = "/v1/library/chapters/"
private const val DOWNLOADED_CHAPTER_PAGE_ROUTE_SEGMENT = "/pages/"
private const val EXTERNAL_READER_IMAGE_ACCEPT = "image/*,*/*;q=0.8"

class MangaDownloader : HttpSource(), ConfigurableSource, UnmeteredSource {
    override fun getName(): String = "Manga Downloader"

    override fun getLang(): String = "all"

    override fun getBaseUrl(): String {
        val configured = preferences().getString(PREF_SERVER_BASE_URL, DEFAULT_SERVER_BASE_URL)
        val trimmed = trimTrailingSlash(configured?.trim().orEmpty())
        return trimmed.ifEmpty { DEFAULT_SERVER_BASE_URL }
    }

    override fun getSupportsLatest(): Boolean = true

    override fun headersBuilder(): Headers.Builder {
        val builder = super.headersBuilder()
        val apiKey = preferences().getString(PREF_BACKEND_API_KEY, "")?.trim().orEmpty()
        if (apiKey.isNotEmpty()) {
            builder.set("Authorization", "Bearer $apiKey")
        }
        return builder
    }

    override fun popularMangaRequest(page: Int): Request = request(apiUrl("v1", "library"))

    override fun popularMangaParse(response: Response): MangasPage {
        val downloaded = downloadedLibrary(parseList(response))
        return MangasPage(toMangaList(downloaded, local = true, sourceName = null), false)
    }

    override fun latestUpdatesRequest(page: Int): Request = popularMangaRequest(page)

    override fun latestUpdatesParse(response: Response): MangasPage = popularMangaParse(response)

    override fun fetchSearchManga(page: Int, query: String?, filters: FilterList?): Observable<MangasPage> {
        val searchQuery = query.orEmpty()
        return Observable.fromCallable(Callable { searchManga(page, searchQuery) })
    }

    override fun searchMangaRequest(page: Int, query: String?, filters: FilterList?): Request {
        return remoteSearchRequest(firstSourceName(), query.orEmpty(), page)
    }

    override fun searchMangaParse(response: Response): MangasPage {
        val sourceName = pathSegment(response.request(), 2)
        val result = parseSearch(response, sourceName)
        return MangasPage(result.mangas, result.hasNextPage)
    }

    override fun mangaDetailsRequest(manga: SManga): Request = request(manga.url)

    override fun mangaDetailsParse(response: Response): SManga {
        val manga = MangaDto.fromJson(JSONObject(response.body().string()))
        val local = pathSegment(response.request(), 1) == "library"
        val sourceName = pathSegment(response.request(), 2)
        return manga.toSManga(this, local, sourceName)
    }

    override fun chapterListRequest(manga: SManga): Request {
        val segments = pathSegments(manga.url)
        if (segments.size > 2 && segments[1] == "library") {
            return request(localMangaChaptersUrl(segments[2]))
        }
        if (segments.size > 4 && segments[1] == "sources") {
            return request(sourceMangaChaptersUrl(segments[2], segments[4]))
        }
        return request(manga.url)
    }

    override fun chapterListParse(response: Response): List<SChapter> {
        val chapters = ChapterDto.listFromJson(JSONObject(response.body().string()).optJSONArray("items"))
        val result = ArrayList<SChapter>()
        if (pathSegment(response.request(), 1) == "library") {
            for (chapter in chapters) {
                result.add(chapter.toSChapter(localChapterPagesUrl(chapter.id), local = true))
            }
        } else if (pathSegment(response.request(), 1) == "sources") {
            val sourceName = pathSegment(response.request(), 2)
            for (chapter in chapters) {
                result.add(chapter.toSChapter(sourceChapterPagesUrl(sourceName, chapter.id), local = false))
            }
        }
        Collections.sort(result) { a, b -> b.chapter_number.compareTo(a.chapter_number) }
        return result
    }

    override fun pageListRequest(chapter: SChapter): Request = request(chapter.url)

    override fun pageListParse(response: Response): List<Page> {
        val pages = JSONObject(response.body().string()).optJSONArray("items") ?: return emptyList()
        val result = ArrayList<Page>()
        for (index in 0 until pages.length()) {
            result.add(Page(index, "", absoluteUrl(pages.optString(index))))
        }
        return result
    }

    override fun imageUrlParse(response: Response): String {
        throw UnsupportedOperationException()
    }

    override fun imageRequest(page: Page): Request {
        return Request.Builder()
            .url(page.imageUrl)
            .headers(headersBuilder().set("Accept", EXTERNAL_READER_IMAGE_ACCEPT).build())
            .build()
    }

    override fun getFilterList(): FilterList = FilterList()

    override fun setupPreferenceScreen(screen: PreferenceScreen) {
        fallbackContext = screen.getContext().applicationContext
        addTextPreference(
            screen,
            PREF_SERVER_BASE_URL,
            "Server Base URL",
            DEFAULT_SERVER_BASE_URL,
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI,
        )
        addTextPreference(
            screen,
            PREF_SOURCE_NAMES,
            "Source Names",
            "Comma-separated source names. Leave blank to auto-discover.",
            InputType.TYPE_CLASS_TEXT,
        )
        addTextPreference(
            screen,
            PREF_BACKEND_API_KEY,
            "Backend API Key",
            "Optional",
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD,
        )
    }

    private fun searchManga(page: Int, query: String): MangasPage {
        val downloaded = downloadedLibrary(executeList(popularMangaRequest(page)))
        val localMatches = ArrayList<SManga>()
        val normalizedQuery = query.trim().lowercase(Locale.ROOT)
        for (manga in downloaded) {
            if (normalizedQuery.isEmpty() || manga.title.lowercase(Locale.ROOT).contains(normalizedQuery)) {
                localMatches.add(manga.toSManga(this, local = true, sourceName = null))
            }
        }
        if (normalizedQuery.isEmpty()) {
            return MangasPage(localMatches, false)
        }

        val index = DownloadedMangaIndex(downloaded)
        val remoteMangas = ArrayList<SManga>()
        var hasNextPage = false
        for (sourceName in sourceNames()) {
            try {
                val result = executeSearch(remoteSearchRequest(sourceName, query, page), sourceName)
                for (manga in result.mangas) {
                    if (!index.contains(sourceName, manga)) {
                        remoteMangas.add(manga)
                    }
                }
                hasNextPage = hasNextPage || result.hasNextPage
            } catch (_: Exception) {
                // Skip sources that are unavailable or do not support this query.
            }
        }

        localMatches.addAll(remoteMangas)
        return MangasPage(localMatches, hasNextPage)
    }

    private fun addTextPreference(screen: PreferenceScreen, key: String, title: String, summary: String, inputType: Int) {
        val preference = EditTextPreference(screen.getContext())
        preference.setKey(key)
        preference.setTitle(title)
        preference.setSummary(summary)
        preference.setOnBindEditTextListener { editText -> editText.inputType = inputType }
        preference.setOnPreferenceChangeListener(Preference.OnPreferenceChangeListener { _, _ ->
            Toast.makeText(screen.getContext(), "Restart Tachiyomi to apply new setting.", Toast.LENGTH_LONG).show()
            true
        })
        screen.addPreference(preference)
    }

    private fun remoteSearchRequest(sourceName: String, query: String, page: Int): Request {
        return request(apiUrl("v1", "sources", sourceName, "search") + "?q=" + urlEncode(query) + "&page=" + page)
    }

    private fun localMangaUrl(mangaId: String): String = apiUrl("v1", "library", mangaId)

    private fun localMangaChaptersUrl(mangaId: String): String = apiUrl("v1", "library", mangaId, "chapters")

    private fun localChapterPagesUrl(chapterId: String): String =
        getBaseUrl() + DOWNLOADED_CHAPTER_PAGE_ROUTE_PREFIX + urlEncode(chapterId) + DOWNLOADED_CHAPTER_PAGE_ROUTE_SEGMENT.removeSuffix("/")

    private fun sourceMangaUrl(sourceName: String, mangaId: String): String = apiUrl("v1", "sources", sourceName, "manga", mangaId)

    private fun sourceMangaChaptersUrl(sourceName: String, mangaId: String): String =
        apiUrl("v1", "sources", sourceName, "manga", mangaId, "chapters")

    private fun sourceChapterPagesUrl(sourceName: String, chapterId: String): String =
        apiUrl("v1", "sources", sourceName, "chapters", chapterId, "pages")

    private fun request(url: String): Request {
        return Request.Builder().url(url).headers(headersBuilder().build()).build()
    }

    private fun executeList(request: Request): List<MangaDto> {
        val response = getClient().newCall(request).execute()
        return try {
            parseList(response)
        } finally {
            response.close()
        }
    }

    private fun executeSearch(request: Request, sourceName: String): RemoteSearchResult {
        val response = getClient().newCall(request).execute()
        return try {
            parseSearch(response, sourceName)
        } finally {
            response.close()
        }
    }

    private fun parseList(response: Response): List<MangaDto> {
        return MangaDto.listFromJson(JSONObject(response.body().string()).optJSONArray("items"))
    }

    private fun parseSearch(response: Response, sourceName: String): RemoteSearchResult {
        val body = JSONObject(response.body().string())
        val mangas = MangaDto.listFromJson(body.optJSONArray("mangas"))
        val result = ArrayList<SManga>()
        for (manga in mangas) {
            result.add(manga.toSManga(this, local = false, sourceName = sourceName))
        }
        return RemoteSearchResult(result, body.optBoolean("has_next_page", false))
    }

    private fun downloadedLibrary(mangas: List<MangaDto>): List<MangaDto> {
        val result = ArrayList<MangaDto>()
        for (manga in mangas) {
            if (manga.downloadedChapters > 0) {
                result.add(manga)
            }
        }
        return result
    }

    private fun toMangaList(mangas: List<MangaDto>, local: Boolean, sourceName: String?): List<SManga> {
        val result = ArrayList<SManga>()
        for (manga in mangas) {
            result.add(manga.toSManga(this, local, sourceName))
        }
        return result
    }

    private fun sourceNames(): List<String> {
        val configured = preferences().getString(PREF_SOURCE_NAMES, "")
        if (!configured.isNullOrBlank()) {
            val result = ArrayList<String>()
            for (name in configured.split(",")) {
                val trimmed = name.trim()
                if (trimmed.isNotEmpty()) {
                    result.add(trimmed)
                }
            }
            if (result.isNotEmpty()) {
                return result
            }
        }

        return try {
            val response = getClient().newCall(request(apiUrl("v1", "sources"))).execute()
            try {
                val items = JSONObject(response.body().string()).optJSONArray("items")
                val result = ArrayList<String>()
                if (items != null) {
                    for (index in 0 until items.length()) {
                        val source = items.optJSONObject(index)
                        if (source != null && source.optBoolean("enabled", false) && contains(source.optJSONArray("capabilities"), "search")) {
                            result.add(source.optString("name"))
                        }
                    }
                }
                result.ifEmpty { defaultSourceNames() }
            } finally {
                response.close()
            }
        } catch (_: Exception) {
            defaultSourceNames()
        }
    }

    private fun firstSourceName(): String {
        val names = sourceNames()
        return names.firstOrNull() ?: DEFAULT_SOURCE_NAMES[0]
    }

    private fun defaultSourceNames(): List<String> = DEFAULT_SOURCE_NAMES.toList()

    private fun preferences(): SharedPreferences {
        val context = applicationContext() ?: return EmptySharedPreferences
        return context.getSharedPreferences("source_${getId()}", 0)
    }

    private fun applicationContext(): Context? {
        fallbackContext?.let { return it }
        return try {
            val injektClass = Class.forName("uy.kohesive.injekt.Injekt")
            val instance = injektClass.getField("INSTANCE").get(null)
            for (method in injektClass.methods) {
                if (method.name == "get" && method.parameterTypes.size == 1) {
                    val value = method.invoke(instance, Context::class.java)
                    if (value is Context) {
                        fallbackContext = value.applicationContext
                        return fallbackContext
                    }
                }
            }
            fallbackContext
        } catch (_: Exception) {
            // Fall back to defaults until the preference screen provides a context.
            fallbackContext
        }
    }

    private fun apiUrl(vararg segments: String): String {
        val builder = StringBuilder(getBaseUrl())
        for (segment in segments) {
            builder.append('/').append(urlEncode(segment))
        }
        return builder.toString()
    }

    private fun absoluteUrl(url: String): String {
        if (url.startsWith("http://") || url.startsWith("https://")) {
            return url
        }
        return if (url.startsWith("/")) getBaseUrl() + url else "${getBaseUrl()}/$url"
    }

    private class MangaDto(json: JSONObject) {
        val id: String = json.optString("id")
        val title: String = json.optString("title")
        private val coverUrl: String = json.optString("cover_url")
        private val coverProxyUrl: String = json.optString("cover_proxy_url", "")
        private val description: String = json.optString("description")
        private val author: String = json.optString("author")
        private val genres: List<String> = stringList(json.optJSONArray("genres"))
        private val status: String = json.optString("status")
        val source: String = json.optString("source")
        val sourceId: String = json.optString("source_id")
        val downloadedChapters: Int = json.optInt("downloaded_chapters", 0)

        fun toSManga(source: MangaDownloader, local: Boolean, sourceName: String?): SManga {
            val manga = SManga.create()
            manga.title = title
            manga.author = author
            manga.description = description
            manga.genre = join(genres)
            manga.status = mangaStatus(status)
            val cover = if (coverProxyUrl.isEmpty()) coverUrl else coverProxyUrl
            manga.thumbnail_url = source.absoluteUrl(cover)
            manga.url = if (local) {
                source.localMangaUrl(id)
            } else {
                source.sourceMangaUrl(sourceName ?: "", id)
            }
            return manga
        }

        companion object {
            fun fromJson(json: JSONObject): MangaDto = MangaDto(json)

            fun listFromJson(array: JSONArray?): List<MangaDto> {
                val result = ArrayList<MangaDto>()
                if (array == null) {
                    return result
                }
                for (index in 0 until array.length()) {
                    val item = array.optJSONObject(index)
                    if (item != null) {
                        result.add(MangaDto(item))
                    }
                }
                return result
            }
        }
    }

    private class ChapterDto(json: JSONObject) {
        val id: String = json.optString("id")
        private val title: String = json.optString("title")
        private val chapterNumber: Float = json.optDouble("chapter_number", -1.0).toFloat()
        private val dateUploaded: String = json.optString("date_uploaded")
        private val pagesRead: Int = json.optInt("pages_read", 0)
        private val readCompleted: Boolean = json.optBoolean("read_completed", false)

        fun toSChapter(url: String, local: Boolean): SChapter {
            val chapter = SChapter.create()
            chapter.url = url
            chapter.name = if (local) readProgressTitle() else if (title.isEmpty()) "Chapter $chapterNumber" else title
            chapter.chapter_number = chapterNumber
            chapter.date_upload = parseTimestamp(dateUploaded)
            return chapter
        }

        private fun readProgressTitle(): String {
            val cleanTitle = title.trim()
            if (readCompleted) {
                return if (cleanTitle.isEmpty()) "Read" else "Read - $cleanTitle"
            }
            if (pagesRead > 0) {
                return if (cleanTitle.isEmpty()) "Page $pagesRead" else "Page $pagesRead - $cleanTitle"
            }
            return cleanTitle
        }

        companion object {
            fun listFromJson(array: JSONArray?): List<ChapterDto> {
                val result = ArrayList<ChapterDto>()
                if (array == null) {
                    return result
                }
                for (index in 0 until array.length()) {
                    val item = array.optJSONObject(index)
                    if (item != null) {
                        result.add(ChapterDto(item))
                    }
                }
                return result
            }
        }
    }

    private class RemoteSearchResult(
        val mangas: List<SManga>,
        val hasNextPage: Boolean,
    )

    private class DownloadedMangaIndex(downloaded: List<MangaDto>) {
        private val sourceIds = HashMap<String, MutableSet<String>>()
        private val titleKeys = HashMap<String, MutableSet<String>>()

        init {
            for (manga in downloaded) {
                add(sourceIds, manga.source, manga.sourceId)
                add(titleKeys, manga.source, normalizedTitle(manga.title))
            }
        }

        fun contains(sourceName: String, manga: SManga): Boolean {
            val ids = sourceIds[sourceName]
            if (ids != null && ids.contains(lastPathSegment(manga.url))) {
                return true
            }
            val titles = titleKeys[sourceName]
            return titles != null && titles.contains(normalizedTitle(manga.title))
        }

        private fun add(map: MutableMap<String, MutableSet<String>>, key: String?, value: String?) {
            if (key.isNullOrEmpty() || value.isNullOrEmpty()) {
                return
            }
            val values = map.getOrPut(key) { HashSet() }
            values.add(value)
        }
    }

    private object EmptySharedPreferences : SharedPreferences {
        override fun getAll(): MutableMap<String, *> = Collections.emptyMap<String, Any>()
        override fun getString(key: String?, defValue: String?): String? = defValue
        override fun getStringSet(key: String?, defValues: MutableSet<String>?): MutableSet<String>? = defValues
        override fun getInt(key: String?, defValue: Int): Int = defValue
        override fun getLong(key: String?, defValue: Long): Long = defValue
        override fun getFloat(key: String?, defValue: Float): Float = defValue
        override fun getBoolean(key: String?, defValue: Boolean): Boolean = defValue
        override fun contains(key: String?): Boolean = false
        override fun edit(): SharedPreferences.Editor {
            throw UnsupportedOperationException()
        }
        override fun registerOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener?) = Unit
        override fun unregisterOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener?) = Unit
    }

    private companion object {
        const val DEFAULT_SERVER_BASE_URL = "http://localhost:4000"
        val DEFAULT_SOURCE_NAMES = arrayOf("comix")

        const val PREF_SERVER_BASE_URL = "serverBaseUrl"
        const val PREF_SOURCE_NAMES = "sourceNames"
        const val PREF_BACKEND_API_KEY = "backendApiKey"

        var fallbackContext: Context? = null

        fun trimTrailingSlash(value: String): String {
            var result = value
            while (result.endsWith("/")) {
                result = result.substring(0, result.length - 1)
            }
            return result
        }

        fun urlEncode(value: String): String {
            return try {
                URLEncoder.encode(value, "UTF-8").replace("+", "%20")
            } catch (_: Exception) {
                value
            }
        }

        fun pathSegment(request: Request, index: Int): String {
            val segments = pathSegments(request.url().toString())
            return if (index < segments.size) segments[index] else ""
        }

        fun pathSegments(url: String): Array<String> {
            var withoutQuery = url.split("?", limit = 2)[0]
            val scheme = withoutQuery.indexOf("://")
            if (scheme >= 0) {
                val path = withoutQuery.indexOf('/', scheme + 3)
                withoutQuery = if (path >= 0) withoutQuery.substring(path) else ""
            }
            return withoutQuery.split("/").toTypedArray()
        }

        fun contains(values: JSONArray?, target: String): Boolean {
            if (values == null) {
                return false
            }
            for (index in 0 until values.length()) {
                if (target == values.optString(index)) {
                    return true
                }
            }
            return false
        }

        fun parseTimestamp(value: String?): Long {
            if (value.isNullOrBlank()) {
                return 0L
            }
            val patterns = arrayOf(
                "yyyy-MM-dd'T'HH:mm:ss.SSSX",
                "yyyy-MM-dd'T'HH:mm:ssX",
                "yyyy-MM-dd HH:mm:ss",
                "yyyy-MM-dd",
            )
            for (pattern in patterns) {
                try {
                    val format = SimpleDateFormat(pattern, Locale.US)
                    format.timeZone = TimeZone.getTimeZone("UTC")
                    return format.parse(value)?.time ?: 0L
                } catch (_: Exception) {
                }
            }
            return 0L
        }

        fun mangaStatus(value: String?): Int {
            return when (value?.lowercase(Locale.ROOT)) {
                "ongoing" -> SManga.ONGOING
                "completed", "complete", "finished" -> SManga.COMPLETED
                else -> SManga.UNKNOWN
            }
        }

        fun normalizedTitle(value: String?): String {
            val result = StringBuilder()
            val lower = value?.lowercase(Locale.ROOT).orEmpty()
            for (char in lower) {
                if (char.isLetterOrDigit()) {
                    result.append(char)
                }
            }
            return result.toString()
        }

        fun stringList(array: JSONArray?): List<String> {
            val result = ArrayList<String>()
            if (array == null) {
                return result
            }
            for (index in 0 until array.length()) {
                val value = array.optString(index)
                if (value.isNotEmpty()) {
                    result.add(value)
                }
            }
            return result
        }

        fun join(values: List<String>): String {
            val result = StringBuilder()
            for (value in values) {
                if (result.isNotEmpty()) {
                    result.append(", ")
                }
                result.append(value)
            }
            return result.toString()
        }

        fun lastPathSegment(url: String): String {
            val segments = pathSegments(url)
            for (index in segments.indices.reversed()) {
                if (segments[index].isNotEmpty()) {
                    return segments[index]
                }
            }
            return ""
        }
    }
}
