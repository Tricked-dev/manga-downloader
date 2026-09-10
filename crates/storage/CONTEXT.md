# Chapter storage

A **Chapter Container** is one BBF file. The `original` section contains native source image bytes after any source-required descrambling. The `upscaled` section, when present, has the same logical page count and order. A page request without a variant selects upscaled when available; explicit original requests always select original. Storage does not resize or re-encode pages.

**Chapter Metadata** includes the ComicInfo XML string and a cover asset reference. The cover is an asset, not a chapter page. Image media types are identified from bytes and recorded per asset.

All file and index operations run on blocking workers. BBF index and asset checksums are verified before serving. A page response retains its memory mapping and shared advisory lock until its last byte owner is dropped. Writers take the exclusive companion-file lock. External tools must cooperate with this lock when modifying a server-owned container.

Original writes stream assets into a temporary file, seal and sync it, then publish it atomically. A failed or canceled write leaves an existing container intact. Upscales use the upstream sealed-file appender. First runs append a second section; reruns replace its page references and append new payloads without rewriting originals. A footer revision check rejects results computed against a changed chapter. ComicInfo updates use metadata upsert through the same appender.
