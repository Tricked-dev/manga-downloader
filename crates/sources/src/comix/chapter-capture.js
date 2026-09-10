(() => {
    if (window.__mangaServerCaptureInstalled) return;
    window.__mangaServerCaptureInstalled = true;
    window.__mangaServerCaptureDone = false;
    window.__mangaServerPayloads = [];

    const originalParse = JSON.parse;
    const pages = new Set();
    const requestedPages = new Set([1]);
    let nextPage;

    const remember = (value, text) => {
        const result = value && (value.result || value.data || value);
        const items = result && (result.items || result.data);
        if (!Array.isArray(items) || (items.length && !items.some(item => item &&
            item.id !== undefined && item.mangaId !== undefined))) return;
        const meta = result.meta || result.pagination || {};
        const page = Number(meta.page || meta.current_page || meta.currentPage || 1);
        if (!Number.isInteger(page) || page < 1 || pages.has(page)) return;
        pages.add(page);
        window.__mangaServerPayloads.push(text);
        const lastPage = Number(meta.lastPage || meta.last_page);
        const hasNext = meta.hasNext ?? meta.has_next ?? (lastPage > page);
        if (hasNext) {
            nextPage = page + 1;
        } else {
            // A late/out-of-order response must not silently produce a partial list.
            window.__mangaServerCaptureDone = Array.from({length: page}, (_, index) => index + 1)
                .every(number => pages.has(number));
            if (window.__mangaServerCaptureDone) clearInterval(interval);
        }
    };
    const chapterUrl = url => {
        try { return new URL(url, location.href).pathname.endsWith('/chapters'); }
        catch (_) { return false; }
    };
    const capture = (url, text) => {
        if (!chapterUrl(url) || typeof text !== 'string') return;
        try { remember(originalParse(text), text); } catch (_) {}
    };

    // Requests are signed by the site; even changing the page size invalidates them.
    const originalOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url, ...rest) {
        this.__mangaServerRequestUrl = String(url);
        return originalOpen.call(this, method, url, ...rest);
    };
    const originalSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function(...args) {
        this.addEventListener('load', () => {
            try { capture(this.responseURL || this.__mangaServerRequestUrl, this.responseText); }
            catch (_) {} // Binary XHR responses do not expose responseText.
        });
        return originalSend.apply(this, args);
    };
    const originalFetch = window.fetch;
    window.fetch = function(input, init) {
        return originalFetch.call(this, input, init).then(response => {
            if (chapterUrl(response.url || String(input?.url || input))) {
                response.clone().text().then(text => capture(response.url || String(input?.url || input), text)).catch(() => {});
            }
            return response;
        });
    };
    // Current Comix responses are decoded by its own client before JSON.parse.
    JSON.parse = new Proxy(originalParse, {
        apply(target, thisArg, args) {
            const value = Reflect.apply(target, thisArg, args);
            try { remember(value, args[0]); } catch (_) {}
            return value;
        }
    });

    const advance = () => {
        if (window.__mangaServerCaptureDone) return;
        const section = document.querySelector('.mpage__chapters');
        if (!section) return;
        section.scrollIntoView({block: 'center'});
        if (!nextPage || requestedPages.has(nextPage)) return;
        const active = section.querySelector('.npager [aria-current]');
        // Allow the site's render to finish before interacting with its next page.
        if (Number(active?.textContent) !== nextPage - 1) return;
        const button = [...section.querySelectorAll('.npager button')]
            .find(button => Number(button.textContent.trim()) === nextPage && !button.disabled);
        if (button) {
            requestedPages.add(nextPage);
            button.click();
        }
    };
    const interval = setInterval(advance, 100);
})();
