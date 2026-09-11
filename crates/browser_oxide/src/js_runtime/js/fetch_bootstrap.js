((globalThis) => {
    const ops = Deno.core.ops;
    const _diagnosticsEnabled = globalThis.__browser_oxide_debug === true;

    // Blob/File are normalized later in binary_fetch_webidl_bootstrap.js,
    // where their backing bytes live in a private WeakMap.  Install a
    // one-shot private hook now so that later layer can inject a synchronous
    // snapshot function into this fetch closure without exposing `_data` on
    // page-visible Blob instances.
    let _blobSnapshotForFetch = null;
    const _blobSnapshotInstallKey = Symbol.for('__browser_oxide_fetch_blob_snapshot__');
    try {
        Object.defineProperty(globalThis, _blobSnapshotInstallKey, {
            value(snapshot) {
                if (typeof snapshot === 'function') _blobSnapshotForFetch = snapshot;
            },
            configurable: true,
            enumerable: false,
            writable: false,
        });
    } catch (_) {}

    const _snapshotBlobForFetch = (value) => {
        if (typeof _blobSnapshotForFetch !== 'function') return null;
        try { return _blobSnapshotForFetch(value); } catch (_) { return null; }
    };
    const _bytesToBody = (bytes) => {
        const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || 0);
        let bin = '';
        for (let i = 0; i < u8.length; i++) bin += String.fromCharCode(u8[i]);
        return 'b:' + btoa(bin);
    };
    const _concatBytes = (chunks) => {
        let length = 0;
        for (const chunk of chunks) length += chunk.byteLength;
        const out = new Uint8Array(length);
        let offset = 0;
        for (const chunk of chunks) { out.set(chunk, offset); offset += chunk.byteLength; }
        return out;
    };
    const _multipartQuoted = (value) => String(value)
        .replace(/\r/g, '%0D')
        .replace(/\n/g, '%0A')
        .replace(/"/g, '%22');

    function _pushFetchDiag(entry) {
        if (!_diagnosticsEnabled) return;
        try {
            const log = globalThis.__oxFetchDiag || (globalThis.__oxFetchDiag = []);
            if (log.length >= 96) log.shift();
            log.push({ ...entry, at: performance.now() });
        } catch (_) {}
    }

    class Headers {
        #map;
        constructor(init = {}) {
            this.#map = {};
            if (init instanceof Headers) {
                init.forEach((v, k) => { this.#map[k] = v; });
            } else if (Array.isArray(init)) {
                for (const [k, v] of init) this.#map[k.toLowerCase()] = v;
            } else if (typeof init === "object") {
                for (const [k, v] of Object.entries(init)) this.#map[k.toLowerCase()] = String(v);
            }
        }
        get(name) { return this.#map[name.toLowerCase()] ?? null; }
        set(name, value) { this.#map[name.toLowerCase()] = String(value); }
        has(name) { return name.toLowerCase() in this.#map; }
        delete(name) { delete this.#map[name.toLowerCase()]; }
        #entriesSorted() {
            return Object.entries(this.#map).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
        }
        forEach(cb, thisArg) {
            for (const [k, v] of this.#entriesSorted()) cb.call(thisArg, v, k, this);
        }
        entries() { return this.#entriesSorted()[Symbol.iterator](); }
        keys() { return this.#entriesSorted().map(([k]) => k)[Symbol.iterator](); }
        values() { return this.#entriesSorted().map(([, v]) => v)[Symbol.iterator](); }
        [Symbol.iterator]() { return this.entries(); }
    }

    class Response {
        #body; #rawBytes; #status; #statusText; #headers; #url; #ok; #bodyStream;
        constructor(body, init = {}) {
            this.#body = body ?? "";
            // Optional authoritative binary payload. When set,
            // arrayBuffer()/blob() hand back the exact bytes without
            // a TextEncoder round-trip — critical for blob: fetches
            // with non-UTF-8 content (e.g. PNG, WASM).
            this.#rawBytes = init._rawBytes ?? null;
            this.#status = init.status ?? 200;
            this.#statusText = init.statusText ?? "OK";
            this.#headers = new Headers(init.headers ?? {});
            this.#url = init.url ?? "";
            this.#ok = this.#status >= 200 && this.#status < 300;
            this.#bodyStream = null;
        }
        get status() { return this.#status; }
        get statusText() { return this.#statusText; }
        get ok() { return this.#ok; }
        get headers() { return this.#headers; }
        get url() { return this.#url; }
        // `body` is a ReadableStream per the Fetch spec. We build it
        // lazily so responses that never read `.body` don't pay for
        // stream construction. The stream yields the cached body as
        // one chunk then closes — enough to satisfy `reader.read()`
        // probes and async-iterator consumption.
        get body() {
            if (this.#bodyStream) return this.#bodyStream;
            if (typeof globalThis.ReadableStream !== "function") return null;
            const raw = this.#rawBytes;
            const textBody = this.#body;
            this.#bodyStream = new globalThis.ReadableStream({
                start(controller) {
                    let bytes = null;
                    if (raw instanceof Uint8Array) {
                        bytes = raw;
                    } else if (textBody != null && textBody !== "") {
                        bytes = new TextEncoder().encode(String(textBody));
                    }
                    if (bytes && bytes.byteLength > 0) controller.enqueue(bytes);
                    controller.close();
                },
            });
            return this.#bodyStream;
        }
        get bodyUsed() {
            return !!this.#bodyStream && this.#bodyStream.locked;
        }
        async text() {
            _pushFetchDiag({ phase: "consume-text", url: this.#url, bodyLength: String(this.#body).length });
            return this.#body;
        }
        async json() { return JSON.parse(this.#body); }
        async arrayBuffer() {
            if (this.#rawBytes && this.#rawBytes.buffer) {
                return this.#rawBytes.buffer.slice(
                    this.#rawBytes.byteOffset,
                    this.#rawBytes.byteOffset + this.#rawBytes.byteLength
                );
            }
            return new TextEncoder().encode(this.#body).buffer;
        }
        async blob() {
            if (this.#rawBytes) {
                const b = new Blob([]);
                b._data = this.#rawBytes;
                b.size = this.#rawBytes.byteLength;
                b.type = this.#headers.get('content-type') || '';
                return b;
            }
            return new Blob([this.#body]);
        }
        clone() {
            return new Response(this.#body, {
                _rawBytes: this.#rawBytes,
                status: this.#status,
                statusText: this.#statusText,
                headers: this.#headers,
                url: this.#url,
            });
        }
    }

    class Request {
        constructor(input, init = {}) {
            if (typeof input === "string") {
                this.url = input;
            } else if (input instanceof Request) {
                this.url = input.url;
                init = { method: input.method, headers: input.headers, body: input.body, ...init };
            }
            this.method = (init.method ?? "GET").toUpperCase();
            this.headers = new Headers(init.headers ?? {});
            this.body = init.body ?? null;
            this._signal = init.signal ?? null;
        }
        // Task#2: real Chrome's Request has a readonly `signal`
        // accessor ON Request.prototype (per the Fetch spec). Defined
        // as a class getter so `Object.hasOwnProperty.call(
        // Request.prototype,"signal")` is true — duolingo's
        // `supportsAbortController` capability gate requires exactly
        // that; without it the homepage self-redirects to
        // /errors/not-supported.html. Lazily backs an AbortSignal so
        // `request.signal` is a non-null AbortSignal like real Chrome.
        get signal() {
            if (this._signal == null
                && typeof globalThis.AbortController === "function") {
                try {
                    this._signal = new globalThis.AbortController().signal;
                } catch (_e) { /* leave null if AbortController throws */ }
            }
            return this._signal;
        }
    }

    // Pull the net::cookies jar snapshot for the current origin into
    // globalThis.__jsCookies so that document.cookie is always in sync.
    // Called after every fetch response and by page.rs after navigation.
    async function _syncCookiesFromNet(url) {
        try {
            url = url || globalThis.location?.href;
            if (!url || url === "about:blank") return;
            const cookieStr = await ops.op_cookie_get(url);
            if (!globalThis.__jsCookies) globalThis.__jsCookies = {};
            if (!cookieStr) return;
            for (const pair of cookieStr.split(";")) {
                const eq = pair.indexOf("=");
                if (eq < 0) continue;
                const k = pair.slice(0, eq).trim();
                const v = pair.slice(eq + 1).trim();
                if (k) globalThis.__jsCookies[k] = v;
            }
        } catch (e) { /* ignore */ }
    }
    globalThis.__syncCookiesFromNet = _syncCookiesFromNet;

    // Normalize any Headers/object/array init into a plain {lower-name: value} object
    function _flattenHeaders(h) {
        const out = {};
        if (!h) return out;
        if (h instanceof Headers) {
            h.forEach((v, k) => { out[k.toLowerCase()] = String(v); });
        } else if (Array.isArray(h)) {
            for (const [k, v] of h) out[String(k).toLowerCase()] = String(v);
        } else if (typeof h === "object") {
            for (const [k, v] of Object.entries(h)) out[String(k).toLowerCase()] = String(v);
        }
        return out;
    }

    function _responseHeaderValue(headers, name) {
        if (!headers || typeof headers !== "object") return null;
        const lower = String(name).toLowerCase();
        for (const [key, value] of Object.entries(headers)) {
            if (String(key).toLowerCase() === lower) return String(value);
        }
        return null;
    }

    // A CORS response exposes only the safelisted response headers plus names
    // authorized by Access-Control-Expose-Headers. The network layer retains
    // the complete header block for protocol/cookie handling, so enforce the
    // renderer-visible guard here before constructing the Response object.
    const _corsSafelistedResponseHeaders = new Set([
        "cache-control",
        "content-language",
        "content-length",
        "content-type",
        "expires",
        "last-modified",
        "pragma",
    ]);
    const _forbiddenResponseHeaders = new Set(["set-cookie", "set-cookie2"]);

    function _stripForbiddenResponseHeaders(headers) {
        const filtered = {};
        for (const [name, value] of Object.entries(headers || {})) {
            const lower = String(name).toLowerCase();
            if (!_forbiddenResponseHeaders.has(lower)) filtered[lower] = String(value);
        }
        return filtered;
    }

    function _corsFilteredResponseHeaders(headers, credentialsMode) {
        const exposed = new Set(_corsSafelistedResponseHeaders);
        const exposeValue = _responseHeaderValue(headers, "access-control-expose-headers") || "";
        let exposeAll = false;
        for (const token of exposeValue.split(",")) {
            const name = token.trim().toLowerCase();
            if (!name) continue;
            // `*` is a wildcard only for responses without credentials. For
            // credentialed responses it denotes a header literally named `*`.
            if (name === "*" && credentialsMode !== "include") exposeAll = true;
            else exposed.add(name);
        }

        const filtered = {};
        for (const [name, value] of Object.entries(headers || {})) {
            const lower = String(name).toLowerCase();
            if (_forbiddenResponseHeaders.has(lower)) continue;
            if (exposeAll || exposed.has(lower)) filtered[lower] = String(value);
        }
        return filtered;
    }

    function _originForUrl(url) {
        try { return new URL(url).origin; } catch (_) { return ""; }
    }

    // Real fetch() using Rust op
    globalThis.fetch = async function fetch(input, init = {}) {
        let url, method, body, headers;

        init = init || {};

        if (typeof input === "string") {
            url = input;
        } else if (input instanceof Request) {
            url = input.url;
            init = { method: input.method, headers: input.headers, body: input.body, ...init };
        } else {
            url = String(input);
        }

        // Resolve relative URLs against current location (like a real browser).
        // Note: URLs like "ftp:" (scheme with no authority) are treated by the
        // URL spec as a relative path component, so `new URL("ftp:", base)`
        // resolves to "<base_origin>/ftp:" — matching real Chrome exactly.
        if (typeof url === "string" && !url.startsWith("http://") && !url.startsWith("https://") && !url.startsWith("data:") && !url.startsWith("blob:")) {
            try {
                const base = globalThis.location?.href || "about:blank";
                if (base && base !== "about:blank") {
                    // Empty url resolves to the document URL itself (real Chrome:
                    // fetch('') hits location.href). The previous `if (url && …)`
                    // guard skipped the empty case, so fetch('') reached op_fetch
                    // unresolved → "relative URL without a base" → a
                    // challenge POST (fetch('') / sendBeacon('')) silently
                    // failed. Our URL polyfill throws on `new URL('',base)`,
                    // so handle empty explicitly.
                    url = (url === "") ? base : new URL(url, base).href;
                }
            } catch (e) { /* keep original URL if resolution fails */ }
        }

        // Reject URLs that still can't be fetched after resolution.
        if (
            url &&
            !url.startsWith("http://") &&
            !url.startsWith("https://") &&
            !url.startsWith("data:") &&
            !url.startsWith("blob:")
        ) {
            throw new TypeError("Failed to fetch");
        }

        // blob: URLs short-circuit the HTTP client: look up the bytes
        // in the Rust BlobRegistry and synthesise a Response. Matches
        // Chrome's `fetch(URL.createObjectURL(blob))` shape — returns
        // 200 with the blob's bytes and content-type.
        if (url && url.startsWith("blob:")) {
            let resp;
            try {
                resp = ops.op_blob_fetch_bytes(url);
            } catch (e) {
                throw new TypeError("Failed to fetch");
            }
            if (!resp || !resp.found) {
                // Unknown blob URL — the spec says a network error.
                throw new TypeError("Failed to fetch");
            }
            // `resp.bytes` comes back from serde as an array of numbers;
            // coerce to Uint8Array so Response.arrayBuffer/blob hand
            // back exact bytes.
            const bytes = resp.bytes instanceof Uint8Array
                ? resp.bytes
                : new Uint8Array(resp.bytes);
            const contentType = resp.content_type || "application/octet-stream";
            // text() on the Response returns UTF-8 decoded bytes so
            // `fetch(blob:URL).text()` works for text blobs; binary
            // blobs should prefer `arrayBuffer()` / `blob()` which
            // use the raw byte path.
            const decoded = new TextDecoder("utf-8", { fatal: false }).decode(bytes);
            return new Response(decoded, {
                _rawBytes: bytes,
                status: 200,
                statusText: "OK",
                headers: { "content-type": contentType },
                url,
            });
        }

        method = (init.method ?? "GET").toUpperCase();
        const requestMode = init.mode == null ? "cors" : String(init.mode);
        const credentialsMode = init.credentials == null ? "same-origin" : String(init.credentials);
        // Body can be: string, ArrayBuffer, TypedArray (Uint8Array), Blob,
        // FormData, URLSearchParams, or null. We must preserve binary
        // fidelity for `application/octet-stream` POSTs.
        //
        // Rust op_fetch accepts the body as a marker-prefixed string:
        //   "s:<text>"   — plain UTF-8 string body
        //   "b:<base64>" — base64-encoded binary body
        const rawBody = init.body;
        // When the body is a FormData, the browser ALWAYS serializes it to
        // multipart/form-data with an auto-generated boundary and sets its
        // own Content-Type (a manually-set Content-Type is ignored for
        // FormData). Track the boundary so we can force the header below.
        let multipartBoundary = null;
        let urlencodedBody = false;
        let bodyKind = 'none';
        let blobContentType = '';
        const _isFormData =
            typeof FormData !== "undefined" && rawBody instanceof FormData;
        const _isUSP =
            typeof URLSearchParams !== "undefined" && rawBody instanceof URLSearchParams;
        if (rawBody == null) {
            body = "";
        } else if (typeof rawBody === "string") {
            body = "s:" + rawBody;
            bodyKind = 'text';
        } else if (rawBody instanceof ArrayBuffer || ArrayBuffer.isView(rawBody)) {
            // BufferSource bodies are byte sequences. Chromium does not
            // synthesize a Content-Type for them.
            const u8 = rawBody instanceof Uint8Array
                ? rawBody
                : new Uint8Array(
                    rawBody.buffer || rawBody,
                    rawBody.byteOffset || 0,
                    rawBody.byteLength,
                );
            body = _bytesToBody(u8);
            bodyKind = 'binary';
        } else if (_isFormData) {
            // Serialize FormData as bytes so File/Blob fields retain arbitrary
            // binary payloads rather than becoming "[object Blob]" strings.
            multipartBoundary =
                "----browserOxideFormBoundary" +
                Math.random().toString(36).slice(2) +
                Math.random().toString(36).slice(2);
            const encoder = new TextEncoder();
            const chunks = [];
            rawBody.forEach((value, name) => {
                chunks.push(encoder.encode("--" + multipartBoundary + "\r\n"));
                const snapshot = typeof Blob !== 'undefined' && value instanceof Blob
                    ? _snapshotBlobForFetch(value)
                    : null;
                if (snapshot) {
                    const filename = snapshot.isFile ? snapshot.name : (value.name || 'blob');
                    const contentType = snapshot.type || 'application/octet-stream';
                    chunks.push(encoder.encode(
                        'Content-Disposition: form-data; name="' + _multipartQuoted(name) +
                        '"; filename="' + _multipartQuoted(filename) + '"\r\n' +
                        'Content-Type: ' + contentType + '\r\n\r\n'
                    ));
                    chunks.push(snapshot.bytes instanceof Uint8Array
                        ? snapshot.bytes
                        : new Uint8Array(snapshot.bytes || 0));
                    chunks.push(encoder.encode('\r\n'));
                } else {
                    chunks.push(encoder.encode(
                        'Content-Disposition: form-data; name="' + _multipartQuoted(name) +
                        '"\r\n\r\n' + String(value) + '\r\n'
                    ));
                }
            });
            chunks.push(encoder.encode("--" + multipartBoundary + "--\r\n"));
            body = _bytesToBody(_concatBytes(chunks));
            bodyKind = 'binary';
        } else if (_isUSP) {
            // URLSearchParams → application/x-www-form-urlencoded.
            body = "s:" + rawBody.toString();
            bodyKind = 'text';
            urlencodedBody = true;
        } else if (typeof Blob !== "undefined" && rawBody instanceof Blob) {
            const snapshot = _snapshotBlobForFetch(rawBody);
            if (snapshot) {
                body = _bytesToBody(snapshot.bytes);
                bodyKind = 'binary';
                blobContentType = String(snapshot.type || '');
            } else {
                // Legacy bootstrap Blob fallback. Page-created normalized Blob
                // objects always take the byte-exact snapshot path above.
                body = "s:" + String(rawBody);
                bodyKind = 'text';
                blobContentType = String(rawBody.type || '');
            }
        } else {
            body = "s:" + String(rawBody);
            bodyKind = 'text';
        }
        headers = _flattenHeaders(init.headers);
        const internalRequestType = headers["x-browser-oxide-request-type"];
        const resourceInitiatorType = internalRequestType === "image"
            ? "img"
            : internalRequestType === "link"
                ? "link"
                : "fetch";

        // FIX-FORMDATA: a FormData body's Content-Type is browser-controlled —
        // FORCE our generated boundary, overriding any (boundaryless)
        // Content-Type the page set, exactly as Chrome does.
        if (multipartBoundary) {
            headers["content-type"] =
                "multipart/form-data; boundary=" + multipartBoundary;
        } else if (urlencodedBody && !headers["content-type"]) {
            headers["content-type"] = "application/x-www-form-urlencoded;charset=UTF-8";
        } else if (blobContentType && !headers["content-type"]) {
            headers["content-type"] = blobContentType;
        } else if (bodyKind === 'text' && body && !headers["content-type"]) {
            // Fetch extracts text/plain only for scalar string-like bodies.
            // BufferSource bodies have no synthesized MIME type.
            headers["content-type"] = "text/plain;charset=UTF-8";
        }

        // Pass the page's origin as a pseudo header so the net layer can
        // compute sec-fetch-site (same-origin vs cross-site) and set Origin /
        // Referer correctly. Chrome's fetch API always carries these.
        try {
            const loc = globalThis.location;
            if (loc && loc.origin && loc.origin !== "null") {
                headers["x-browser-oxide-origin"] = loc.origin;
                // Fetch Metadata: Chromium includes
                // `Sec-Fetch-Storage-Access` on modern fetch/XHR requests.
                // Keep this as an engine-private pseudo header; the Rust op
                // strips it before the request reaches the wire and inserts
                // the real browser-controlled header in Chromium order.
                let storageAccess = "active";
                try {
                    const site = (value) => {
                        const parsed = new URL(String(value), loc.href);
                        const host = parsed.hostname.toLowerCase();
                        if (!host || /^\d+(?:\.\d+){3}$/.test(host) || host.indexOf('.') < 0) {
                            return parsed.protocol + "//" + host;
                        }
                        const labels = host.split('.');
                        let take = 2;
                        if (labels.at(-1).length === 2
                            && /^(?:ac|co|com|edu|gov|net|org)$/.test(labels.at(-2))) {
                            take = 3;
                        }
                        return parsed.protocol + "//" + labels.slice(-take).join('.');
                    };
                    const ownerSite = site(loc.href);
                    const ancestors = loc.ancestorOrigins;
                    for (let i = 0; ancestors && i < ancestors.length; i++) {
                        if (site(ancestors[i]) !== ownerSite) {
                            storageAccess = "none";
                            break;
                        }
                    }
                } catch (_) {}
                headers["x-browser-oxide-storage-access"] = storageAccess;
                if (!headers["referer"]) {
                    let targetOrigin = "";
                    try { targetOrigin = new URL(url).origin; } catch (_) {}
                    // Chromium's default strict-origin-when-cross-origin
                    // policy sends the full document URL for same-origin
                    // fetch/XHR/image requests and only the origin otherwise.
                    headers["referer"] = targetOrigin === loc.origin
                        ? String(loc.href || loc.origin).replace(/#.*$/, "")
                        : loc.origin.replace(/\/$/, "") + "/";
                }
            } else if (loc && loc.href && loc.href !== "about:blank") {
                const u = new URL(loc.href);
                headers["x-browser-oxide-origin"] = u.origin;
                headers["x-browser-oxide-storage-access"] = "active";
                if (!headers["referer"]) {
                    headers["referer"] = u.href.replace(/#.*$/, "");
                }
            }
        } catch {}

        try {
            const startTime = performance.now();
            let documentOrigin = "";
            try { documentOrigin = String(globalThis.location?.origin || ""); } catch (_) {}
            const requestOrigin = _originForUrl(url);
            const crossOrigin = !!documentOrigin
                && documentOrigin !== "null"
                && !!requestOrigin
                && requestOrigin !== documentOrigin;
            if (requestMode === "same-origin" && crossOrigin) {
                throw new TypeError("Failed to fetch");
            }
            _pushFetchDiag({
                phase: "request",
                method,
                url,
                rawBodyType: rawBody == null
                    ? String(rawBody)
                    : Object.prototype.toString.call(rawBody),
                rawBodyLength: rawBody && typeof rawBody.length === "number"
                    ? rawBody.length
                    : rawBody && typeof rawBody.byteLength === "number"
                        ? rawBody.byteLength
                        : null,
                encodedBodyLength: body.length,
                headerNames: Object.keys(headers).sort(),
            });
            // Preserve renderer request-header order across the JS -> Rust
            // boundary. Serializing a plain object into a Rust HashMap made
            // page-authored headers nondeterministic on the wire, while real
            // browsers keep a stable network-stack header order. The op takes
            // an ordered list of [name, value] pairs so the net layer can
            // merge replacements in-place without losing that order.
            const result = await ops.op_fetch(url, method, Object.entries(headers), body);
            _pushFetchDiag({
                phase: "response",
                method,
                url,
                status: result.status,
                bodyLength: result.body ? result.body.length : 0,
                headers: Object.entries(result.headers || {})
                    .map(([name, value]) => [name, String(value).length])
                    .sort((a, b) => a[0].localeCompare(b[0])),
            });

            // Fetch CORS is a renderer-side response gate. The network stack
            // still performs the request, but a cross-origin `mode: "cors"`
            // promise rejects unless Access-Control-Allow-Origin authorizes
            // the calling realm. Challenge workers deliberately probe this:
            // exposing a readable 204 response where Chrome reports
            // `TypeError: Failed to fetch` changes their proof branch.
            if (result.status === 0) {
                throw new TypeError("Failed to fetch");
            }
            if (crossOrigin && requestMode === "cors") {
                const allowedOrigin = _responseHeaderValue(
                    result.headers,
                    "access-control-allow-origin",
                );
                const allowsCredentials = _responseHeaderValue(
                    result.headers,
                    "access-control-allow-credentials",
                );
                const wildcardAllowed = allowedOrigin === "*" && credentialsMode !== "include";
                const explicitOriginAllowed = allowedOrigin === documentOrigin
                    && (credentialsMode !== "include" || allowsCredentials === "true");
                if (!wildcardAllowed && !explicitOriginAllowed) {
                    throw new TypeError("Failed to fetch");
                }
            }
            if (crossOrigin && requestMode === "no-cors") {
                return new Response("", {
                    status: 0,
                    statusText: "",
                    headers: {},
                    url: "",
                });
            }
            
            const browser_oxide = globalThis._browser_oxide;
            if (_diagnosticsEnabled && result.body && result.body.length > 10000) {
                globalThis.__oxLastLargeFetchBody = result.body;
                globalThis.__oxLastLargeFetchUrl = url;
            }
            const fetchLog = browser_oxide && browser_oxide.__fetchLog;
            if (fetchLog) {
                fetchLog.push({ method, url, status: result.status });
            }

            const entries = browser_oxide && browser_oxide.__perfResourceEntries;
            if (entries) {
                const entry = {
                    url,
                    type: resourceInitiatorType,
                    startTime,
                    duration: performance.now() - startTime,
                    size: result.decoded_body_size || 0,
                    transferSize: result.transfer_size || 0,
                    encodedBodySize: result.encoded_body_size || 0,
                    decodedBodySize: result.decoded_body_size || 0,
                };
                entries.push(entry);
                try {
                    const notify = globalThis[Symbol.for('__browser_oxide_performance_resource__')];
                    if (typeof notify === 'function') notify(entry);
                } catch (_) {}
            }

            // Sync cookies from the net jar into document.cookie so subsequent JS
            // reads (including challenge polling loops) see Set-Cookie
            // values that arrived via this response.
            await _syncCookiesFromNet(url);
            const responseHeaders = crossOrigin && requestMode === "cors"
                ? _corsFilteredResponseHeaders(result.headers, credentialsMode)
                : _stripForbiddenResponseHeaders(result.headers);
            return new Response(result.body, {
                _rawBytes: result.body_bytes ? new Uint8Array(result.body_bytes) : null,
                status: result.status,
                statusText: result.status_text,
                headers: responseHeaders,
                url: result.url,
            });
        } catch (e) {
            // A CSP gate may have rejected the request inside op_fetch and
            // queued a SecurityPolicyViolationEvent payload. Chromium makes
            // that event observable as part of the blocked request lifecycle;
            // drain before exposing the generic Fetch network error so page
            // listeners do not depend on bootstrap-time polling timers.
            try { _drainCspViolations(); } catch (_) {}
            // Log error for audit
            const browser_oxide = globalThis._browser_oxide;
            const fetchLog = browser_oxide && browser_oxide.__fetchLog;
            if (fetchLog) {
                fetchLog.push({ method, url, status: 0, error: e.message });
            }
            if (e instanceof TypeError && e.message === "Failed to fetch") throw e;
            // Fetch deliberately hides transport details from page script.  In
            // Chromium a DNS, TCP, TLS, or HTTP-stack failure is exposed only
            // as the standard network-error TypeError; the underlying reason
            // remains available in our internal fetch log above.
            throw new TypeError("Failed to fetch");
        }
    };

    globalThis.Headers = Headers;
    globalThis.Response = Response;
    globalThis.Request = Request;

    // CSP violation drain — pulls violations the Rust gates queued and
    // dispatches `securitypolicyviolation` events for each on document
    // and window. Real Chrome dispatches the event synchronously at
    // the moment of block; we batch + drain because our gates run
    // off-OpState. Drain on a few timed checkpoints so listeners
    // installed early in page lifecycle catch the events.
    function _drainCspViolations() {
        try {
            const violations = ops.op_drain_csp_violations();
            if (!violations || !violations.length) return;
            for (const v of violations) {
                let ev;
                try {
                    ev = new SecurityPolicyViolationEvent("securitypolicyviolation", {
                        bubbles: true,
                        cancelable: false,
                        blockedURI: v.blockedURI,
                        violatedDirective: v.violatedDirective,
                        effectiveDirective: v.effectiveDirective,
                        disposition: v.disposition,
                    });
                } catch (_) {
                    // Fallback if SecurityPolicyViolationEvent is unavailable.
                    ev = new CustomEvent("securitypolicyviolation", {
                        bubbles: true, cancelable: false, detail: v,
                    });
                }
                try { if (globalThis.document) globalThis.document.dispatchEvent(ev); } catch (_) {}
                try { globalThis.dispatchEvent(ev); } catch (_) {}
                if (globalThis.console && typeof console.error === 'function') {
                    console.error(
                        "Refused to load '" + v.blockedURI +
                        "' because it violates the following Content Security Policy directive: \"" +
                        v.effectiveDirective + "\"."
                    );
                }
            }
        } catch (_) { /* best-effort */ }
    }

    Promise.resolve().then(_drainCspViolations);
    if (typeof setTimeout === "function") {
        setTimeout(_drainCspViolations, 0);
        setTimeout(_drainCspViolations, 50);
        setTimeout(_drainCspViolations, 250);
    }
    Object.defineProperty(globalThis, "__drainCspViolations", {
        value: _drainCspViolations, enumerable: false, configurable: true,
    });

    // Mask Function.prototype.toString so scripts that inspect it
    // see `function fetch() { [native code] }` (as in real Chrome)
    // instead of our literal source.
    if (typeof globalThis._maskFunction === 'function') {
        try { globalThis._maskFunction(globalThis.fetch, 'fetch'); } catch (_) {}
        if (globalThis.Request) try { globalThis._maskFunction(globalThis.Request, 'Request'); } catch (_) {}
        if (globalThis.Response) try { globalThis._maskFunction(globalThis.Response, 'Response'); } catch (_) {}
        if (globalThis.Headers) try { globalThis._maskFunction(globalThis.Headers, 'Headers'); } catch (_) {}
    }
})(globalThis);
