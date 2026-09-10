# Manga Downloader

Manga Downloader is a self-hosted manga reading and download system. This glossary names the domain concepts used when discussing source plugins, the local library, downloads, and reader behaviour.

## Language

**Chapter Page**:
One ordered readable page within a chapter. A Chapter Page may be backed by a source plugin media reference or by a downloaded archive entry, but callers should not treat URL, image, or archive entry as the reader-facing concept.
_Avoid_: image, page URL, media ref, archive entry

**Source Chapter Page**:
A Chapter Page whose bytes are obtained from a source plugin. A Source Chapter Page belongs to a source chapter, may require source-specific fetch behaviour, and can be used as fallback when downloaded backing is unavailable.
_Avoid_: remote image, proxied URL

**Downloaded Chapter Page**:
A Chapter Page whose bytes are obtained from the local library's downloaded archive. When both source and downloaded backing are available for the same Local Library Chapter, the Downloaded Chapter Page is preferred.
_Avoid_: archive entry, extracted image

**Local Library Chapter**:
A chapter tracked in the local library for a saved manga. A Local Library Chapter may have read progress, may be downloaded, or may still be read through source plugin pages.
_Avoid_: saved chapter, tracked chapter

**Read Progress**:
The furthest Chapter Page reached for a Local Library Chapter, plus whether the chapter was completed in a reading session. Read Progress belongs to the Local Library Chapter, not to source or downloaded backing, and is a best-known position rather than immutable page-count truth.
_Avoid_: source progress, download progress

**Library Update Run**:
A check across Local Library manga for newly available source chapters. A Library Update Run may be scheduled or manually triggered, may baseline a newly initialized series, may enqueue Downloads for new Local Library Chapters, and may emit notifications.
_Avoid_: scheduler tick, chapter sync loop

**Chapter Page Reference**:
An ordered reference to a Chapter Page that can be read during a reader session. A Chapter Page Reference is not durable identity, is not page bytes, and is not necessarily a URL, though external clients may receive URL-shaped adapters for compatibility.
_Avoid_: page URL, image URL, raw media reference

**Chapter Page Warming**:
Preparing nearby Chapter Pages so the reader can display them with less waiting. Chapter Page Warming may cross to the next Local Library Chapter when local library ordering identifies one, and is defined in terms of Chapter Pages rather than source URLs, cache entries, or archive extraction details.
_Avoid_: image prefetch, cache priming, archive pre-extract

**Source Catalog**:
The installed and configured set of source plugins available for searching, browsing, and reading Source Chapter Pages. Source Catalog changes include plugin install, reload, deletion, enablement, and source-level settings that affect source visibility or behavior.
_Avoid_: plugin list, source cache

**Download**:
A queue or work record for fetching a chapter into local storage. A Download may be queued, running, failed, canceled, or completed.
_Avoid_: archive, downloaded chapter

**Download Work State**:
The persisted state and public label for a Download work record as it moves through queueing, fetching, conversion, archiving, cancellation, failure, or completion. Download Work State belongs to the Download work record; creating or removing a durable local reading artifact is Downloaded Archive Lifecycle.
_Avoid_: stage string, ad hoc download status

**Downloaded Archive**:
The durable local reading artifact produced by a completed Download. A Downloaded Archive can be indexed, deleted, reencoded, and read as Downloaded Chapter Pages.
_Avoid_: download record, archive file

**Downloaded Archive Index**:
Derived state for a Downloaded Archive that records ordered page-entry facts needed to read Downloaded Chapter Pages. It can be rebuilt or invalidated without changing the Downloaded Archive itself.
_Avoid_: extraction queue, page cache, read-ahead state

**Archive Index Maintenance**:
Operations that inspect, rebuild, clear, or remove stale Downloaded Archive Index rows, then coordinate the repairable derived-state effects needed after that maintenance. Archive Index Maintenance changes index state; it does not create, delete, or reencode Downloaded Archives.
_Avoid_: settings maintenance, route cleanup

**Downloaded Archive Lifecycle**:
The set of domain changes that create, delete, or reencode a Downloaded Archive. Deletion means a previously completed Downloaded Archive stopped being the durable local artifact for a Local Library Chapter; failed download cleanup, retry, cancellation, and queue record removal are Download lifecycle concerns unless they remove a completed archive. Reencoding means the same durable artifact changed representation in place, not that one Downloaded Archive was deleted and another was created. The Downloaded Archive remains valid even when derived state, such as an index, must be rebuilt later.
_Avoid_: download lifecycle, file maintenance

**Route Snapshot**:
A short-lived cached response for an HTTP route. A Route Snapshot is operational derived state and can be discarded whenever a domain change makes the cached response possibly stale.
_Avoid_: domain cache, permanent view

**Route Snapshot Invalidation**:
Discarding affected Route Snapshots after a domain change. Callers should name the domain change they performed, not the route names that may be stale.
_Avoid_: cache clear, common invalidation

**Settings Interface**:
The application-facing interface for persisted configuration. It owns setting keys, defaults, parsing, runtime fallbacks, and change classification so callers ask for typed values rather than reading raw setting strings.
_Avoid_: raw setting map, ad hoc setting parse

**Stats Projection**:
A read model that summarizes the local library, downloads, source coverage, reading activity, and attached operational summaries. Current totals come from current state; activity over time comes from recorded activity facts.
_Avoid_: stats scan, analytics API

**Chapter Read**:
An activity fact recorded when a Local Library Chapter becomes complete. It is not recorded for every page view.
_Avoid_: page read event

**Chapter Pages Downloaded**:
An activity fact recording how many Chapter Pages became locally available when a Downloaded Archive was created.
_Avoid_: archive page count event

**Chapter Downloaded**:
An activity fact recorded when a Downloaded Archive is created for one Local Library Chapter. Reencoding a Downloaded Archive does not create another Chapter Downloaded fact.
_Avoid_: download completed event

## Example Dialogue

Developer: "Should the reader ask for image URLs?"

Domain expert: "No. The reader asks for Chapter Pages. Some Chapter Pages come from source plugins, and some come from downloaded archives."

Developer: "Should source reading and downloaded reading use different reader concepts?"

Domain expert: "No. They are both Chapter Pages. Source Chapter Pages and Downloaded Chapter Pages differ by backing, not by what the reader is asking to display."

Developer: "If both backing kinds exist, which should the reader use?"

Domain expert: "Prefer the Downloaded Chapter Page. Use the Source Chapter Page as fallback when downloaded backing is unavailable."

Developer: "If I read a chapter through source pages and download it later, does the progress split?"

Domain expert: "No. Read Progress belongs to the Local Library Chapter, so changing the backing does not create a second progress record."

Developer: "If source and downloaded backing have different page counts, does completion mean both are complete?"

Domain expert: "No. The active backing defines the reading session's page count. Read Progress records the best-known position and completion from that session."

Developer: "When listing pages, should the module return image URLs?"

Domain expert: "It should return Chapter Page References. Reading a Chapter Page Reference produces renderable page content."

Developer: "Should we store Chapter Page References as permanent IDs?"

Domain expert: "No. Local Library Chapter plus page index is the durable identity; Chapter Page References are for reading sessions."

Developer: "Should callers know how source pages and downloaded pages are warmed?"

Domain expert: "No. They ask for Chapter Page Warming around the current reading position; the backing determines the details."

Developer: "Can warming jump from one chapter to the next?"

Domain expert: "Yes, when the next Local Library Chapter is known from local library ordering."

Developer: "Is a completed download the same thing as the archive on disk?"

Domain expert: "No. The Download is the work record. The Downloaded Archive is the durable local artifact created when that work completes."

Developer: "If indexing fails after a download completes, is the archive invalid?"

Domain expert: "No. The Downloaded Archive exists; the derived state needs to be repaired."

Developer: "Is the Downloaded Archive Index the same thing as page cache or read-ahead state?"

Domain expert: "No. The Downloaded Archive Index records page-entry facts for the Downloaded Archive. Page cache and read-ahead state are operational reader infrastructure."

Developer: "Are stats only current database counts?"

Domain expert: "No. Stats Projection combines current totals with activity over time."

Developer: "Does reencoding a downloaded archive count as downloading a chapter again?"

Domain expert: "No. Chapter Downloaded is recorded when the Downloaded Archive is created, not when its representation changes."

Developer: "Does clearing cache change reading history?"

Domain expert: "No. Cache is an operational summary attached to Stats Projection, not a reading or download activity fact."

Developer: "Should a route handler know every cached route snapshot affected by a Downloaded Archive change?"

Domain expert: "No. It should report the domain change. Route Snapshot Invalidation maps that change to operational cached responses."
