package eu.kanade.tachiyomi.extension.all.mangadownloader

import eu.kanade.tachiyomi.source.Source
import eu.kanade.tachiyomi.source.SourceFactory

class MangaDownloaderFactory : SourceFactory {
    override fun createSources(): List<Source> = listOf(MangaDownloader())
}
