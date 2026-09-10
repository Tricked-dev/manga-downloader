pub const DONE_EXPRESSION: &str = "window.__mangaServerCaptureDone";
pub const PAYLOADS_EXPRESSION: &str = "window.__mangaServerPayloads";
pub const TIMEOUT_MS: u32 = 75_000;
pub const POLL_INTERVAL_MS: u32 = 500;

pub const SEARCH_SCRIPT: &str = r"
(() => {
    if (window.__mangaServerCaptureInstalled) return;
    window.__mangaServerCaptureInstalled = true;
    window.__mangaServerCaptureDone = false;
    window.__mangaServerPayloads = [];

    const seen = new Set();

    const searchUrl = (url) => {
        try {
            const parsed = new URL(url, location.href);
            return parsed.origin === location.origin &&
                parsed.pathname === '/api/v1/manga' &&
                !parsed.pathname.includes('/chapters');
        } catch (_) {
            return false;
        }
    };

    const looksLikeSearchPayload = (value) => {
        const result = value && (value.result || value.data || value);
        const items = result && (result.items || result.data);
        if (!Array.isArray(items)) return false;
        if (items.length === 0) return true;
        return items.some((item) => item &&
            typeof item === 'object' &&
            (item.title || item.hid || item.poster) &&
            (item.id !== undefined || item.mangaId !== undefined));
    };

    const remember = (url, text) => {
        if (!searchUrl(url) || typeof text !== 'string' || seen.has(text)) return;
        try {
            if (!looksLikeSearchPayload(JSON.parse(text))) return;
            seen.add(text);
            window.__mangaServerPayloads.push(text);
            window.__mangaServerCaptureDone = true;
        } catch (_) {}
    };

    const originalOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url, ...rest) {
        this.__mangaServerRequestUrl = String(url || '');
        return originalOpen.call(this, method, url, ...rest);
    };

    const originalSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function(...args) {
        this.addEventListener('load', () => {
            remember(this.responseURL || this.__mangaServerRequestUrl, this.responseText);
        });
        return originalSend.apply(this, args);
    };

    const originalFetch = window.fetch;
    window.fetch = function(input, init) {
        const requestUrl = typeof input === 'string' ? input : input && input.url;
        return originalFetch.call(this, input, init).then((response) => {
            const url = response.url || requestUrl || '';
            if (searchUrl(url)) {
                response.clone().text().then((text) => remember(url, text)).catch(() => {});
            }
            return response;
        });
    };
})();
";

pub const CHAPTER_LIST_SCRIPT: &str = r#"
(() => {
    if (window.__mangaServerCaptureInstalled) return;
    window.__mangaServerCaptureInstalled = true;
    window.__mangaServerCaptureDone = false;
    window.__mangaServerPayloads = [];

    const rewriteUrl = (url) => {
        if (typeof url === 'string' && url.includes('/chapters')) {
            if (/[?&]limit=\d+/.test(url)) {
                return url.replace(/([?&]limit=)\d+/, '$15000');
            }
            return `${url}${url.includes('?') ? '&' : '?'}limit=5000`;
        }
        return url;
    };

    const numberFrom = (...values) => {
        for (const value of values) {
            const number = Number(value);
            if (Number.isFinite(number) && number > 0) return number;
        }
        return undefined;
    };

    const seen = new Set();

    const chapterUrl = (url) => {
        try {
            return new URL(url, location.href).pathname.includes('/chapters');
        } catch (_) {
            return false;
        }
    };

    const looksLikeChapterPayload = (value) => {
        const result = value && (value.result || value.data || value);
        const items = result && (result.items || result.data);
        if (!Array.isArray(items)) return false;
        if (items.length === 0) return true;
        return items.some((item) => item &&
            typeof item === 'object' &&
            item.id !== undefined &&
            (item.mangaId !== undefined || item.number !== undefined));
    };

    const remember = (url, text) => {
        if (!chapterUrl(url) || typeof text !== 'string' || seen.has(text)) return;
        try {
            if (!looksLikeChapterPayload(JSON.parse(text))) return;
            seen.add(text);
            window.__mangaServerPayloads.push(text);
            window.__mangaServerCaptureDone = true;
        } catch (_) {}
    };

    const originalOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url, ...rest) {
        const rewritten = rewriteUrl(url);
        this.__mangaServerRequestUrl = String(rewritten || '');
        return originalOpen.call(this, method, rewritten, ...rest);
    };

    const originalSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function(...args) {
        this.addEventListener('load', () => {
            remember(this.responseURL || this.__mangaServerRequestUrl, this.responseText);
        });
        return originalSend.apply(this, args);
    };

    const originalFetch = window.fetch;
    window.fetch = function(input, init) {
        let requestUrl = typeof input === 'string' ? input : input && input.url;
        if (typeof input === 'string') {
            input = rewriteUrl(input);
            requestUrl = input;
        }
        if (input && typeof input.url === 'string') {
            const rewritten = rewriteUrl(input.url);
            if (rewritten !== input.url) {
                input = new Request(rewritten, input);
                requestUrl = rewritten;
            }
        }
        return originalFetch.call(this, input, init).then((response) => {
            const url = response.url || requestUrl || '';
            if (chapterUrl(url)) {
                response.clone().text().then((text) => remember(url, text)).catch(() => {});
            }
            return response;
        });
    };

    const originalParse = JSON.parse;
    JSON.parse = new Proxy(originalParse, {
        apply(target, thisArg, args) {
            const parsed = Reflect.apply(target, thisArg, args);
            try {
                if (
                    parsed && parsed.result &&
                    Array.isArray(parsed.result.items) &&
                    parsed.result.items.length > 0 &&
                    parsed.result.items[0] &&
                    parsed.result.items[0].id !== undefined &&
                    parsed.result.items[0].mangaId !== undefined
                ) {
                    const meta = parsed.result.meta || parsed.result.pagination || {};
                    const page = numberFrom(meta.page, meta.current_page, meta.currentPage) || 1;
                    if (!seen.has(page)) {
                        seen.add(page);
                        window.__mangaServerPayloads.push(args[0]);
                        window.__mangaServerCaptureDone = true;
                    }
                }
            } catch (_) {}
            return parsed;
        }
    });

    let scrollAttempts = 0;
    const nudgeChapterSection = () => {
        if (window.__mangaServerCaptureDone || scrollAttempts >= 80) {
            clearInterval(scrollInterval);
            return;
        }
        scrollAttempts += 1;
        const chapterElement = document.querySelector(
            '[id*="chapter" i], [class*="chapter" i], a[href*="/chapter"]'
        );
        try {
            if (chapterElement) {
                chapterElement.scrollIntoView({ block: 'center' });
            } else {
                window.scrollTo(0, document.body ? document.body.scrollHeight : 0);
            }
        } catch (_) {
            window.scrollTo(0, document.body ? document.body.scrollHeight : 0);
        }
    };
    const scrollInterval = setInterval(nudgeChapterSection, 250);
    nudgeChapterSection();
})();
"#;

pub const PAGE_LIST_SCRIPT: &str = r#"
(() => {
    if (window.__mangaServerCaptureInstalled) return;
    window.__mangaServerCaptureInstalled = true;
    window.__mangaServerCaptureDone = false;
    window.__mangaServerPayloads = [];
    window.__mangaServerImageUrls = [];
    window.__mangaServerPlainImageUrls = [];
    window.__mangaServerDecodedCapturePending = true;
    window.__mangaServerOriginalCanvasGetImageData = CanvasRenderingContext2D.prototype.getImageData;

    const imageUrlPattern = /\.(?:avif|jpe?g|png|webp)(?:[?#].*)?$/i;
    const numberedImagePattern = /^(.*\/)(\d+)(\.(?:avif|jpe?g|png|webp)(?:[?#].*)?)$/i;
    const grid = 5;

    const rememberImageUrl = (url) => {
        if (typeof url !== 'string' || !imageUrlPattern.test(url)) return;
        if (!window.__mangaServerImageUrls.includes(url)) {
            window.__mangaServerImageUrls.push(url);
        }
    };

    const isScrambled = (value) => value === 1 || value === true || value === '1' || value === 'true';

    const absoluteOrJoinedUrl = (baseUrl, path) => {
        if (typeof path !== 'string') return '';
        if (/^https?:\/\//i.test(path)) return path;
        if (!baseUrl) return path;
        return `${baseUrl.replace(/\/+$/, '')}/${path.replace(/^\/+/, '')}`;
    };

    const tileUrl = (url) => url.replace(/\/sii\/(?=[bh])/, '/si/').replace(/\/i\/(?=[bh])/, '/si/');

    const assetUrls = () => [...document.querySelectorAll('script[src],link[href]')]
        .map((element) => element.src || element.href)
        .filter((url) => typeof url === 'string' && url.endsWith('.js'));

    const moduleUrl = (needle) => assetUrls().find((url) => url.includes(needle));

    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

    const waitForModuleUrl = async (...needles) => {
        const started = Date.now();
        while (Date.now() - started < 15000) {
            for (const needle of needles) {
                const url = moduleUrl(needle);
                if (url) return url;
            }
            await sleep(250);
        }
        return undefined;
    };

    const chapterIdFromInitialData = () => {
        const candidates = [
            document.querySelector('#initial-data'),
            document.querySelector('script#__NEXT_DATA__'),
            document.querySelector('script[type="application/json"]'),
        ].filter(Boolean);
        const findChapterId = (value, depth = 0) => {
            if (!value || depth > 8) return undefined;
            if (Array.isArray(value)) {
                for (const item of value) {
                    const found = findChapterId(item, depth + 1);
                    if (found) return found;
                }
                return undefined;
            }
            if (typeof value === 'object') {
                for (const [key, item] of Object.entries(value)) {
                    if (/^chapter_?id$/i.test(key)) {
                        const number = Number(item);
                        if (Number.isFinite(number) && number > 0) return String(number);
                    }
                    const found = findChapterId(item, depth + 1);
                    if (found) return found;
                }
            }
            return undefined;
        };

        for (const element of candidates) {
            try {
                const parsed = JSON.parse(element.textContent || '');
                const found = findChapterId(parsed);
                if (found) return found;
            } catch (_) {}
        }
        return undefined;
    };

    const chapterIdFromLocation = () => {
        const match = location.pathname.match(/\/(\d+)(?:-[^/]*)?\/?$/);
        return match ? match[1] : undefined;
    };

    const normalizePages = (chapter) => {
        const pages = chapter && (chapter.pages || (chapter.result && chapter.result.pages));
        const images = chapter && (chapter.images || (chapter.result && chapter.result.images));
        const normalizePage = (item, baseUrl = '') => {
            if (typeof item === 'string') {
                return { url: absoluteOrJoinedUrl(baseUrl, item) };
            }
            if (!item || typeof item !== 'object') return undefined;

            const scrambled = isScrambled(item.s) || isScrambled(item.scramble) || isScrambled(item.scrambled);
            const rawUrl = item.url || item.src || item.path;
            let url = absoluteOrJoinedUrl(baseUrl, rawUrl);
            if (scrambled) {
                url = tileUrl(url);
            }
            if (!url) return undefined;
            return {
                url,
                s: scrambled ? 1 : 0,
                width: item.width,
                height: item.height,
            };
        };

        if (Array.isArray(images)) return images.map((item) => normalizePage(item)).filter(Boolean);
        if (Array.isArray(pages)) return pages.map((item) => normalizePage(item)).filter(Boolean);
        if (!pages || !Array.isArray(pages.items)) return [];

        const baseUrl = pages.baseUrl || pages.base_url || '';
        const tileBaseUrl = tileUrl(baseUrl);
        return pages.items
            .map((item) => {
                if (typeof item === 'string') {
                    return { url: absoluteOrJoinedUrl(baseUrl, item) };
                }
                if (!item || typeof item !== 'object') return undefined;
                const scrambled = isScrambled(item.s) || isScrambled(item.scramble) || isScrambled(item.scrambled);
                let url = absoluteOrJoinedUrl(scrambled ? tileBaseUrl : baseUrl, item.url || item.src || item.path);
                if (scrambled) {
                    url = tileUrl(url);
                }
                if (!url) return undefined;
                return {
                    url,
                    s: scrambled ? 1 : 0,
                    width: item.width,
                    height: item.height,
                };
            })
            .filter(Boolean);
    };

    const tileEdge = (position, dimension) => Math.floor((dimension * position + Math.floor(grid / 2)) / grid);
    const tileBox = (index, width, height) => {
        const column = index % grid;
        const row = Math.floor(index / grid);
        const x = tileEdge(column, width);
        const y = tileEdge(row, height);
        const right = tileEdge(column + 1, width);
        const bottom = tileEdge(row + 1, height);
        return { x, y, width: Math.max(0, right - x), height: Math.max(0, bottom - y) };
    };

    const loadImage = (url) => new Promise((resolve, reject) => {
        const image = new Image();
        image.crossOrigin = 'anonymous';
        image.onload = () => resolve(image);
        image.onerror = () => reject(new Error(`Failed to load image ${url}`));
        image.src = url;
    });

    const canvasImageData = (context, x, y, width, height) => {
        const getImageData = window.__mangaServerOriginalCanvasGetImageData || CanvasRenderingContext2D.prototype.getImageData;
        return getImageData.call(context, x, y, width, height);
    };

    const pixelOffset = (data, width, x, y) => ((y * width) + x) * 4;

    const tileCost = (renderedData, rawData, width, height, destinationIndex, sourceIndex) => {
        const destination = tileBox(destinationIndex, width, height);
        const source = tileBox(sourceIndex, width, height);
        const samplesX = Math.max(2, Math.min(24, destination.width, source.width));
        const samplesY = Math.max(2, Math.min(24, destination.height, source.height));
        let cost = 0;
        let samples = 0;
        for (let y = 0; y < samplesY; y += 1) {
            for (let x = 0; x < samplesX; x += 1) {
                const dx = destination.x + Math.min(destination.width - 1, Math.floor(((x + 0.5) * destination.width) / samplesX));
                const dy = destination.y + Math.min(destination.height - 1, Math.floor(((y + 0.5) * destination.height) / samplesY));
                const sx = source.x + Math.min(source.width - 1, Math.floor(((x + 0.5) * source.width) / samplesX));
                const sy = source.y + Math.min(source.height - 1, Math.floor(((y + 0.5) * source.height) / samplesY));
                const destinationOffset = pixelOffset(renderedData.data, renderedData.width, dx, dy);
                const sourceOffset = pixelOffset(rawData.data, rawData.width, sx, sy);
                cost += Math.abs(renderedData.data[destinationOffset] - rawData.data[sourceOffset]);
                cost += Math.abs(renderedData.data[destinationOffset + 1] - rawData.data[sourceOffset + 1]);
                cost += Math.abs(renderedData.data[destinationOffset + 2] - rawData.data[sourceOffset + 2]);
                samples += 1;
            }
        }
        return samples ? cost / samples : Number.POSITIVE_INFINITY;
    };

    const validateMap = (map) => {
        if (!Array.isArray(map) || map.length !== 25) return false;
        const seen = new Set(map);
        return seen.size === 25 && map.every((value) => Number.isInteger(value) && value >= 0 && value < 25);
    };

    const identityMap = () => Array.from({ length: 25 }, (_, index) => index);

    const averageMapCost = (costs, map) => {
        let total = 0;
        for (let destination = 0; destination < 25; destination += 1) {
            total += costs[destination][map[destination]];
        }
        return total / 25;
    };

    const meaningfulMap = (costs, map) => {
        const identity = identityMap();
        if (map.every((value, index) => value === index)) return undefined;

        const identityCost = averageMapCost(costs, identity);
        const assignedCost = averageMapCost(costs, map);
        const absoluteImprovement = identityCost - assignedCost;
        const relativeImprovement = identityCost > 0 ? absoluteImprovement / identityCost : 0;

        if (absoluteImprovement >= 8 && relativeImprovement >= 0.18) return map;
        return undefined;
    };

    const solveAssignment = (costs) => {
        const n = costs.length;
        const u = new Array(n + 1).fill(0);
        const v = new Array(n + 1).fill(0);
        const p = new Array(n + 1).fill(0);
        const way = new Array(n + 1).fill(0);

        for (let i = 1; i <= n; i += 1) {
            p[0] = i;
            let j0 = 0;
            const minv = new Array(n + 1).fill(Number.POSITIVE_INFINITY);
            const used = new Array(n + 1).fill(false);
            do {
                used[j0] = true;
                const i0 = p[j0];
                let delta = Number.POSITIVE_INFINITY;
                let j1 = 0;
                for (let j = 1; j <= n; j += 1) {
                    if (used[j]) continue;
                    const cur = costs[i0 - 1][j - 1] - u[i0] - v[j];
                    if (cur < minv[j]) {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if (minv[j] < delta) {
                        delta = minv[j];
                        j1 = j;
                    }
                }
                for (let j = 0; j <= n; j += 1) {
                    if (used[j]) {
                        u[p[j]] += delta;
                        v[j] -= delta;
                    } else {
                        minv[j] -= delta;
                    }
                }
                j0 = j1;
            } while (p[j0] !== 0);

            do {
                const j1 = way[j0];
                p[j0] = p[j1];
                j0 = j1;
            } while (j0 !== 0);
        }

        const assignment = new Array(n);
        for (let j = 1; j <= n; j += 1) {
            assignment[p[j] - 1] = j - 1;
        }
        return assignment;
    };

    const deriveDescrambleMap = async (url, renderSecurePage) => {
        const renderUrl = tileUrl(url);
        const rawImage = await loadImage(renderUrl);
        const width = rawImage.naturalWidth || rawImage.width;
        const height = rawImage.naturalHeight || rawImage.height;
        if (!width || !height) return undefined;

        const rawCanvas = document.createElement('canvas');
        rawCanvas.width = width;
        rawCanvas.height = height;
        const rawContext = rawCanvas.getContext('2d', { willReadFrequently: true });
        rawContext.drawImage(rawImage, 0, 0);

        const renderedCanvas = document.createElement('canvas');
        renderedCanvas.width = width;
        renderedCanvas.height = height;
        const abortController = new AbortController();
        await renderSecurePage(renderUrl, renderedCanvas, abortController.signal);
        if (renderedCanvas.width !== width || renderedCanvas.height !== height) {
            renderedCanvas.width = width;
            renderedCanvas.height = height;
            await renderSecurePage(renderUrl, renderedCanvas, abortController.signal);
        }

        const renderedContext = renderedCanvas.getContext('2d', { willReadFrequently: true });
        const rawData = canvasImageData(rawContext, 0, 0, width, height);
        const renderedData = canvasImageData(renderedContext, 0, 0, width, height);
        const costs = [];
        for (let destination = 0; destination < 25; destination += 1) {
            const row = [];
            for (let source = 0; source < 25; source += 1) {
                row.push(tileCost(renderedData, rawData, width, height, destination, source));
            }
            costs.push(row);
        }
        const map = solveAssignment(costs);
        return validateMap(map) ? meaningfulMap(costs, map) : undefined;
    };

    const captureDecodedPayload = async () => {
        const envUrl = await waitForModuleUrl('/env-', 'env-');
        if (!envUrl) return;

        const envModule = await import(envUrl);
        const client = Object.values(envModule).find((value) => (
            value && typeof value.get === 'function' && typeof value.post === 'function'
        ));
        if (!client) return;

        const chapterId = chapterIdFromInitialData() || chapterIdFromLocation();
        if (!chapterId) return;

        const chapter = await client.get(`/chapters/${chapterId}`);
        const pages = normalizePages(chapter);
        if (!pages.length) return;

        const secureUrl = await waitForModuleUrl('/secure-', 'secure-');
        const secureModule = secureUrl ? await import(secureUrl) : {};
        const renderSecurePage = typeof secureModule.t === 'function'
            ? secureModule.t
            : Object.values(secureModule).find((value) => typeof value === 'function' && value.length >= 2);

        if (typeof renderSecurePage === 'function') {
            for (const page of pages) {
                if (!isScrambled(page.s)) continue;
                try {
                    const map = await deriveDescrambleMap(page.url, renderSecurePage);
                    if (map) page.manga_server_descramble_map = map;
                } catch (_) {}
            }
        }

        window.__mangaServerPayloads.unshift(JSON.stringify({ result: { images: pages } }));
        window.__mangaServerCaptureDone = true;
    };

    const totalFromReader = () => {
        const progress = document.querySelectorAll('.rpage-progress__seg');
        if (progress.length > 0) return progress.length;

        const text = document.body && document.body.innerText ? document.body.innerText : '';
        let best = 0;
        const re = /\b\d+\s*\/\s*(\d{1,4})\b/g;
        let match;
        while ((match = re.exec(text))) {
            const value = Number(match[1]);
            if (Number.isFinite(value) && value > best) best = value;
        }
        return best;
    };

    const observedImageUrls = () => {
        const urls = [...window.__mangaServerImageUrls];
        for (const img of document.images) {
            const url = img.currentSrc || img.src;
            rememberImageUrl(url);
            if (typeof url === 'string' && imageUrlPattern.test(url) && !window.__mangaServerPlainImageUrls.includes(url)) {
                window.__mangaServerPlainImageUrls.push(url);
            }
        }
        for (const url of window.__mangaServerImageUrls) {
            if (!urls.includes(url)) urls.push(url);
        }
        return urls;
    };

    const synthesizePagePayload = () => {
        if (window.__mangaServerDecodedCapturePending) return;
        if (window.__mangaServerPayloads.length > 0) return;

        const total = totalFromReader();
        if (!Number.isFinite(total) || total <= 0) return;

        const seed = observedImageUrls().find((url) => numberedImagePattern.test(url));
        if (!seed) return;

        const match = numberedImagePattern.exec(seed);
        if (!match) return;

        const [, prefix, digits, suffix] = match;
        const width = digits.length;
        const items = [];
        for (let index = 1; index <= total; index += 1) {
            const url = `${prefix}${String(index).padStart(width, '0')}${suffix}`;
            const plain = window.__mangaServerPlainImageUrls.includes(url);
            items.push({
                url,
                ...(plain ? {} : { s: 1 }),
            });
        }

        window.__mangaServerPayloads.push(JSON.stringify({ result: { images: items } }));
        window.__mangaServerCaptureDone = true;
    };

    const originalFetch = window.fetch;
    window.fetch = function(input, init) {
        const url = typeof input === 'string' ? input : input && typeof input.url === 'string' ? input.url : '';
        rememberImageUrl(url);
        return originalFetch.call(this, input, init);
    };

    const originalOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url, ...rest) {
        rememberImageUrl(url);
        return originalOpen.call(this, method, url, ...rest);
    };

    const originalParse = JSON.parse;
    JSON.parse = new Proxy(originalParse, {
        apply(target, thisArg, args) {
            const parsed = Reflect.apply(target, thisArg, args);
            try {
                if (parsed && parsed.result && parsed.result.pages) {
                    const pages = normalizePages(parsed);
                    const hasScrambledPages = pages.some((page) => isScrambled(page.s));
                    if (!hasScrambledPages && window.__mangaServerPayloads.length === 0) {
                        window.__mangaServerPayloads.push(JSON.stringify({ result: { images: pages } }));
                        window.__mangaServerCaptureDone = true;
                    }
                }
            } catch (_) {}
            return parsed;
        }
    });

    const interval = setInterval(() => {
        try {
            synthesizePagePayload();
            if (window.__mangaServerCaptureDone) clearInterval(interval);
        } catch (_) {}
    }, 250);

    captureDecodedPayload()
        .catch(() => {})
        .finally(() => {
            window.__mangaServerDecodedCapturePending = false;
        });
})();
"#;
