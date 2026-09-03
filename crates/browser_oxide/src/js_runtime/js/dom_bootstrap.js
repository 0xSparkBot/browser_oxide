((globalThis) => {
    const core = Deno.core;
    const ops = core.ops;
    const _nodeIds = new WeakMap();
    const _nodeCache = new Map();
    const _scrollState = new Map(); // nodeId -> {top, left}
    // Per-node type/tag cached in WeakMaps (not on the node) so
    // `Object.getOwnPropertyNames(el)` stays [] like real Chrome.
    const _nodeTypes = new WeakMap();
    const _tagNames = new WeakMap();
    const _localNames = new WeakMap();
    const _shadowRoots = new WeakMap();
    const _shadowHosts = new WeakMap();
    const _shadowModes = new WeakMap();
    const _shadowRootsByHostNode = new Map();
    const _shadowHostNodeByRootNode = new Map();
    const _shadowModeByRootNode = new Map();
    const _closedFrameIds = new Set();
    let _markFrameMessageTrusted = null;

    function _debugFrameLifecycle(entry) {
        if (!globalThis.__browser_oxide_debug) return;
        try {
            if (!Array.isArray(globalThis.__oxideFrameDebug)) globalThis.__oxideFrameDebug = [];
            globalThis.__oxideFrameDebug.push(entry);
            if (globalThis.__oxideFrameDebug.length > 200) globalThis.__oxideFrameDebug.shift();
        } catch (_) {}
    }

    function _getNodeId(node) {
        if (node === null || node === undefined) return -1;
        if (node === globalThis || node === globalThis.window) return -999;
        // WeakMap.get on a non-object returns undefined per spec — no throw.
        const id = _nodeIds.get(node);
        if (id === undefined) {
            // node is not a registered DOM node. Returning 0 (the DOCUMENT
            // id) here used to be a "resilience" default, but it caused
            // every appendChild(weirdValue) to surface as
            // appendChild(parent, document) → cycle assertion fires.
            // -1 makes the Rust op layer's `dom.get(NodeId(u32::MAX))` miss
            // and silently no-op, which is the right behaviour for a JS
            // mutation against a non-node argument.
            return -1;
        }
        return id;
    }

    function _wrapNode(nodeId) {
        if (nodeId === null || nodeId === undefined || nodeId === -1) return null;
        // Callers like the iframe-shadow `getElementsByTagName` mirrors hand
        // us a wrapped node instead of its id; resolve it here so the smi
        // op below always receives a number (`expected i32` otherwise).
        if (typeof nodeId !== "number") {
            nodeId = _getNodeId(nodeId);
            if (nodeId === -1) return null;
        }
        const cached = _nodeCache.get(nodeId);
        if (cached) {
            const obj = cached.deref();
            if (obj) return obj;
        }
        const nodeType = ops.op_dom_get_node_type(nodeId);
        return _wrapNodeWithType(nodeId, nodeType);
    }

    function _wrapNodeWithType(nodeId, nodeType) {
        if (nodeId === null || nodeId === undefined || nodeId === -1) return null;
        const cached = _nodeCache.get(nodeId);
        if (cached) {
            const obj = cached.deref();
            if (obj) return obj;
        }
        let node;
        switch (nodeType) {
            case 1:
                node = new Element(nodeId);
                _retargetElementProto(node);
                break;
            case 3: node = new Text(nodeId); break;
            case 8: node = new Comment(nodeId); break;
            case 9: node = _document; break;
            case 11: node = new DocumentFragment(nodeId); break;
            default: node = new Node(nodeId); break;
        }
        _nodeTypes.set(node, nodeType);
        _nodeCache.set(nodeId, new WeakRef(node));
        return node;
    }

    // Tracks base URLs (query-stripped) of scripts currently being sync-fetched.
    // Guards against re-entrant fetch loops: e.g. Yandex Metrika's bootstrap IIFE
    // inserts a new <script src="tag.js?timestamp"> while tag.js is still being
    // evaluated. Without this guard the fetch recurses infinitely.
    const _syncFetchInFlight = new Set();

    // Tracks nesting depth of sync eval chains. Each _onNodeInserted call that
    // fetches+evals a script increments this. Scripts beyond MAX nesting are
    // degraded to async — prevents C++ stack overflow when deeply-nested
    // third-party SDKs load more scripts during their own synchronous eval
    // (each pending eval adds a large V8 interpreter frame to the C stack;
    // 6-9 levels can overflow an 8 MB Rust thread stack).
    let _syncEvalDepth = 0;
    const _MAX_SYNC_EVAL_DEPTH = 4;

    // Guards against unbounded `document.write` chains. Two failure modes
    // we observed on bot.sannysoft.com:
    //   (a) A script does `document.write('<script>...</script>')` and the
    //       written script does the same — direct cycle. Caught by depth.
    //   (b) `document.write` dispatches every new node through
    //       `_onNodeInserted`, which evals scripts. If a written script
    //       calls `document.write` again during its eval (synchronously),
    //       we re-enter `_onNodeInserted` from inside its own call.
    let _onNodeInsertedDepth = 0;
    const _MAX_NODE_INSERT_DEPTH = 64;

    // Bound by the iframe subsystem later in bootstrap. Frame-indexed access
    // (`window[n]`/`frames[n]`) must return the exact same public WindowProxy as
    // `iframe.contentWindow`, never the transport/backend object directly.
    let _publicWindowProxyFor = function(_el, backend) { return backend; };

    // Frame-tree routing handle when the frame tree is active and the frame is
    // materialized, else the same-isolate child realm.
    function _frameWindowFor(el) {
        let backend = null;
        try {
            if (globalThis.__frameId !== undefined && globalThis.__frameIdForNode && globalThis.__frameHandleFor) {
                const _f = globalThis.__frameIdForNode[_getNodeId(el)];
                if (_f !== undefined) backend = globalThis.__frameHandleFor(_f);
            }
        } catch (_) {}
        if (!backend) backend = _getIframeWindow(el);
        return _publicWindowProxyFor(el, backend);
    }

    function _onNodeInserted(child, sync = true) {
        if (!child) return;
        if (_onNodeInsertedDepth >= _MAX_NODE_INSERT_DEPTH) {
            // Bail — log once and skip. This breaks document.write recursion
            // chains that would otherwise blow the C-stack via deep nested
            // eval -> op_dom_document_write -> _onNodeInserted.
            console.log(`[DOM] _onNodeInserted depth limit (${_MAX_NODE_INSERT_DEPTH}) — skipping`);
            return;
        }
        _onNodeInsertedDepth++;
        try {
            const insertedNodeId = _getNodeId(child);
            // The Rust target collector is optimized for executable/loadable
            // descendants and does not guarantee that an ordinary shadow host
            // appears in the returned list. Check the actual inserted node
            // explicitly so a prebuilt shadow tree becomes active when its host
            // first enters the document.
            _activateConnectedShadowTree(child, sync);
            _activateShadowHostsInSubtree(child, sync);
            const targets = ops.op_dom_collect_insert_targets(insertedNodeId);
            for (let i = 0; i < targets.length; i++) {
                const el = _wrapNode(targets[i]);
                if (el) {
                    _handleInsertedElement(el, sync);
                    try {
                        if (_getNodeId(el) !== insertedNodeId) {
                            _activateConnectedShadowTree(el, sync);
                            _activateShadowHostsInSubtree(el, sync);
                        }
                    } catch (_) {}
                }
            }
        } finally {
            _onNodeInsertedDepth--;
        }
    }

    function _activateConnectedShadowTree(host, sync = true) {
        let root = null;
        try {
            root = _shadowRoots.get(host) || null;
            if (!root) root = _shadowRootsByHostNode.get(_getNodeId(host)) || null;
        } catch (_) {}
        if (!root) return;
        let connected = false;
        try { connected = !!host.isConnected; } catch (_) {}
        if (!connected) return;

        // Shadow-root descendants are not part of the host's ordinary DOM
        // child list, so op_dom_collect_insert_targets(host) cannot see them.
        // Activate each direct shadow child; _onNodeInserted then covers its
        // light descendants and recursively discovers nested shadow hosts.
        try {
            const children = root.childNodes;
            for (let i = 0; i < children.length; i++) {
                _onNodeInserted(children[i], sync);
            }
        } catch (_) {}
    }

    // Widget builders (e.g. Turnstile) assemble an iframe inside a closed
    // shadow root while the host is still detached, then commit the host as
    // part of a larger subtree. op_dom_collect_insert_targets only walks the
    // light DOM, so an iframe nested in a shadow tree of a detached host is
    // invisible at commit time. Walk the inserted subtree and activate every
    // shadow host that is now connected; nested hosts activate recursively via
    // _activateConnectedShadowTree -> _onNodeInserted.
    function _activateShadowHostsInSubtree(el, sync = true, depth = 0) {
        if (!el || depth > 32) return;
        try {
            if (el.nodeType === 1 && _shadowRootsByHostNode.has(_getNodeId(el))) {
                _activateConnectedShadowTree(el, sync);
            }
        } catch (_) {}
        try {
            let child = el.firstElementChild;
            while (child) {
                _activateShadowHostsInSubtree(child, sync, depth + 1);
                child = child.nextElementSibling;
            }
        } catch (_) {}
    }

    class DOMPointReadOnly {
        constructor(x = 0, y = 0, z = 0, w = 1) {
            this.x = x; this.y = y; this.z = z; this.w = w;
        }
        static fromPoint(p) { return new DOMPointReadOnly(p.x, p.y, p.z, p.w); }
        toJSON() { return { x: this.x, y: this.y, z: this.z, w: this.w }; }
    }
    globalThis.DOMPointReadOnly = DOMPointReadOnly;

    class DOMPoint extends DOMPointReadOnly {
        constructor(x = 0, y = 0, z = 0, w = 1) { super(x, y, z, w); }
    }
    globalThis.DOMPoint = DOMPoint;

    class DOMRectReadOnly {
        constructor(x = 0, y = 0, width = 0, height = 0) {
            this.x = x; this.y = y; this.width = width; this.height = height;
        }
        get top() { return this.y; }
        get left() { return this.x; }
        get right() { return this.x + this.width; }
        get bottom() { return this.y + this.height; }
        toJSON() { return { x: this.x, y: this.y, width: this.width, height: this.height, top: this.top, left: this.left, right: this.right, bottom: this.bottom }; }
    }
    globalThis.DOMRectReadOnly = DOMRectReadOnly;

    class DOMRect extends DOMRectReadOnly {
        constructor(x = 0, y = 0, width = 0, height = 0) { super(x, y, width, height); }
        static fromRect(r) { return new DOMRect(r.x, r.y, r.width, r.height); }
    }
    globalThis.DOMRect = DOMRect;

    if (typeof _maskFunction === 'function') {
        _maskFunction(DOMPointReadOnly, 'DOMPointReadOnly');
        _maskFunction(DOMPoint, 'DOMPoint');
        _maskFunction(DOMRectReadOnly, 'DOMRectReadOnly');
        _maskFunction(DOMRect, 'DOMRect');
    }

    function _handleInsertedElement(child, sync = true) {
        // 1. Dynamic script loading
        const childTag = (child.tagName || child.nodeName || "").toLowerCase();
        const type = (child.getAttribute?.('type') || '').toLowerCase();
        const isJs = !type || type === 'text/javascript' || type === 'application/javascript' || type === 'module';
        
        if (childTag === 'script' && !isJs) {
            return; // Skip non-JS scripts like application/ld+json
        }

        const childSrc = (childTag === 'script') ? (child.src || child.getAttribute?.('src')) : null;

        if (childTag === 'script' && !childSrc) {
            const code = child.textContent || child.innerText || '';
            if (code && code.trim()) {
                if (_currentMatRealmId !== null) {
                    // Inserted by a child realm's code — run it in that realm.
                    try { ops.op_eval_in_child_realm(_currentMatRealmId, code); } catch (_) {}
                } else {
                    console.log(`[DOM] executing inline script (${code.length} bytes)`);
                    try { (0, eval)(code); } catch (e) {
                        console.log(`[DOM] inline eval error: ${e.message}`);
                    }
                }
            }
        }

        if (childTag === 'script' && childSrc) {
            const src = childSrc;
            const scriptEl = child;

            // Script inserted by a child realm's code: resolve its relative src
            // against the frame's origin and run it in that realm.
            if (_currentMatRealmId !== null) {
                const _rid = _currentMatRealmId;
                let _cbase = _realmBaseUrl.get(_rid) || (globalThis.location ? globalThis.location.href : 'about:blank');
                let _cUrl = src;
                if (!src.startsWith('http') && !src.startsWith('data:')) {
                    try { _cUrl = new URL(src, _cbase).href; } catch (_) {}
                }
                try {
                    const _ccode = ops.op_net_fetch_frame_sync(_cUrl, _cbase);
                    if (_ccode) {
                        try { ops.op_eval_in_child_realm(_rid, _ccode); } catch (_) {}
                        if (scriptEl.onload) scriptEl.onload(new Event('load'));
                        scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('load'));
                    } else {
                        if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                        scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                    }
                } catch (_) {
                    if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                    scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                }
                return;
            }

            let fullUrl = src;
            if (!src.startsWith('http') && !src.startsWith('data:')) {
                try {
                    const base = globalThis.location ? globalThis.location.href : 'about:blank';
                    fullUrl = new URL(src, base).href;
                } catch(e) {}
            }

            // Third-party trackers known to trigger uncontrolled C-stack recursion
            // inside their own VM (not in our shims). Skip them — they add no
            // signal to fingerprint scoring, and crashing the engine on them
            // costs us all subsequent tests on the page.
            // Known offenders identified via stack-overflow crashes on real
            // sites: bot.sannysoft.com loads Yandex Metrika; leboncoin.fr
            // loads it too.
            const _RECURSIVE_TRACKERS = [
                "mc.yandex.ru/metrika/tag.js",
                "mc.yandex.ru/metrika/watch.js",
                "mc.yandex.ru/webvisor/",
            ];
            for (const pat of _RECURSIVE_TRACKERS) {
                if (fullUrl.includes(pat)) {
                    if (scriptEl.onload) scriptEl.onload(new Event('load'));
                    scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('load'));
                    return;
                }
            }

            if (sync) {
                // Strip query params for in-flight dedup: scripts that reload themselves
                // with a cache-busting timestamp (e.g. Yandex Metrika tag.js?<timestamp>)
                // share the same base URL and would recurse infinitely without this guard.
                const baseUrl = fullUrl.split('?')[0];
                if (_syncFetchInFlight.has(baseUrl)) {
                    // Re-entrant same-URL fetch — fire load event and bail to break the cycle.
                    if (scriptEl.onload) scriptEl.onload(new Event('load'));
                    scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('load'));
                    return;
                }
                // Depth guard: if sync evals are already nested beyond the safe limit,
                // degrade to async. This prevents C++ stack overflow from chains like
                // tag.js → pixel.js → tracker.js → … where each level blocks the V8
                // thread inside op_net_fetch_sync while its eval frame stays on stack.
                if (_syncEvalDepth >= _MAX_SYNC_EVAL_DEPTH) {
                    console.log(`[DOM] sync eval depth limit (${_MAX_SYNC_EVAL_DEPTH}) — falling back to async: ${fullUrl}`);
                    (async () => {
                        try {
                            const resp = await globalThis.fetch(fullUrl);
                            if (resp.ok) {
                                const code = await resp.text();
                                try { (0, eval)(code); } catch(_) {}
                                if (scriptEl.onload) scriptEl.onload(new Event('load'));
                                scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('load'));
                            }
                        } catch(_) {
                            if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                            scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                        }
                    })();
                    return;
                }
                _syncFetchInFlight.add(baseUrl);
                _syncEvalDepth++;
                console.log(`[DOM] sync fetching script (depth ${_syncEvalDepth}): ${fullUrl}`);
                try {
                    // A child frame's scripts belong to the frame's origin (its
                    // own CSP), so use the fetch that skips the top's script-src.
                    const _isChildFrame = globalThis.__frameId !== undefined
                        && globalThis.__frameId !== globalThis.__topFrameId;
                    const code = _isChildFrame
                        ? ops.op_net_fetch_frame_sync(fullUrl, globalThis.location?.href || "")
                        : ops.op_net_fetch_sync(fullUrl, globalThis.location?.href || "");
                    if (code) {
                        console.log(`[DOM] sync executing script (${code.length} bytes): ${fullUrl}`);
                        try {
                            (0, eval)(code);
                            console.log(`[DOM] sync execution SUCCESS: ${fullUrl}`);
                        } catch(e) {
                            console.log(`[DOM] sync eval ERROR for ${fullUrl}: ${e.message}\n${e.stack}`);
                            if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                            scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                        }
                    } else {
                        console.log(`[DOM] sync fetch FAILED (empty) for ${fullUrl}`);
                        if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                        scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                    }
                    if (scriptEl.onload) scriptEl.onload(new Event('load'));
                    scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('load'));
                } catch(e) {
                    console.log(`[DOM] sync fetch OP error for ${fullUrl}: ${e.message}`);
                    if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                    scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                } finally {
                    _syncFetchInFlight.delete(baseUrl);
                    _syncEvalDepth--;
                }
            } else {
                console.log(`[DOM] async fetching script: ${fullUrl}`);
                (async () => {
                    try {
                        const resp = await globalThis.fetch(fullUrl);
                        if (resp.ok) {
                            const code = await resp.text();
                            console.log(`[DOM] async executing script (${code.length} bytes): ${fullUrl}`);
                            try {
                                (0, eval)(code);
                                console.log(`[DOM] async execution SUCCESS: ${fullUrl}`);
                                if (scriptEl.onload) scriptEl.onload(new Event('load'));
                                scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('load'));
                            } catch(e) {
                                console.log(`[DOM] async eval ERROR for ${fullUrl}: ${e.message}\n${e.stack}`);
                                if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                                scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                            }
                        } else {
                            console.log(`[DOM] async fetch FAILED (status ${resp.status}) for ${fullUrl}`);
                            if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                            scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                        }
                    } catch(e) {
                        console.log(`[DOM] async fetch ERROR for ${fullUrl}: ${e.message}`);
                        if (scriptEl.onerror) scriptEl.onerror(new Event('error'));
                        scriptEl.dispatchEvent && scriptEl.dispatchEvent(new Event('error'));
                    }
                })();
            }
        }

        // Fire `load` on an injected stylesheet/preload <link>: CSS chunk loaders
        // wait on it or hang. Deferred to a microtask to match async loading.
        if (childTag === 'link') {
            let _rel = '';
            try { _rel = String((child.getAttribute && child.getAttribute('rel')) || child.rel || '').toLowerCase(); } catch (_) {}
            let _lhref = '';
            try { _lhref = (child.getAttribute && child.getAttribute('href')) || child.href || ''; } catch (_) {}
            if (_lhref && (_rel.indexOf('stylesheet') >= 0 || _rel.indexOf('preload') >= 0 || _rel.indexOf('prefetch') >= 0 || _rel.indexOf('modulepreload') >= 0)) {
                queueMicrotask(() => {
                    try {
                        if (typeof child.onload === 'function') child.onload(new Event('load'));
                        child.dispatchEvent && child.dispatchEvent(new Event('load'));
                    } catch (_) {}
                });
            }
        }

        // Auto-load <iframe src>: build the child realm so it can postMessage back.
        // Deferred to a task so it runs after the inserting script sets up listeners.
        if (childTag === 'iframe') {
            // A disconnected iframe has no child navigable. Nodes assembled in
            // DocumentFragment/template/off-document containers must stay inert
            // until the insertion that actually connects them to a document.
            // Registering here would allocate a frame id before contentWindow
            // exists and leave stale node->frame mappings behind.
            let _connected = false;
            try { _connected = !!child.isConnected; } catch (_) {}
            try {
                _debugFrameLifecycle({
                    phase: 'insert', nodeId: _getNodeId(child), connected: _connected,
                    src: (child.getAttribute && child.getAttribute('src')) || '',
                    parentRealm: child.__oxParentRealm,
                });
            } catch (_) {}
            if (!_connected) return;
            let isrc = '';
            try { isrc = (child.getAttribute && child.getAttribute('src')) || child.src || ''; } catch (_) {}
            // Record the realm inserting this iframe (null = top page) so its
            // parent-postMessage routing chains correctly once materialized.
            try { if (child.__oxParentRealm === undefined) child.__oxParentRealm = _currentMatRealmId; } catch (_) {}
            const _sameIsolateParent = typeof child.__oxParentRealm === 'number';
            if (isrc && isrc !== 'about:blank' && !/^javascript:/i.test(isrc) && !/^data:/i.test(isrc)) {
                // Only top/frame-tree runtimes emit a Rust pending-frame signal.
                // A frame created inside a same-isolate child realm must stay in
                // that hierarchy; attaching its detached DOM host to the top
                // frame tree would give it the wrong parent/origin.
                if (!_sameIsolateParent) {
                    try {
                        const _abs = new URL(isrc, (globalThis.location && globalThis.location.href) || 'about:blank').href;
                        let _nm = '';
                        try { _nm = (child.getAttribute && child.getAttribute('name')) || child.name || ''; } catch (_) {}
                        const _nodeId = _getNodeId(child);
                        const _existingFid = globalThis.__frameIdForNode && globalThis.__frameIdForNode[_nodeId];
                        if (_existingFid !== undefined) return;
                        const _fid = ops.op_frame_pending(_nodeId, _abs, _nm);
                        if (_fid) {
                            _debugFrameLifecycle({phase:'pending-insert',nodeId:_nodeId,fid:_fid,src:_abs});
                            let _childOrigin = "null";
                            try { _childOrigin = new URL(_abs).origin; } catch (_) {}
                            globalThis.__oxRegisterChildFrame(_nodeId, _fid, _childOrigin);
                        }
                    } catch (_) {}
                }
                if (globalThis.__frameId === undefined || _sameIsolateParent) {
                    setTimeout(() => { try { void child.contentWindow; } catch (_) {} }, 0);
                }
            } else {
                // about:blank/srcdoc child browsing contexts are created even
                // without an explicit contentWindow read.
                setTimeout(() => { try { void child.contentWindow; } catch (_) {} }, 0);
            }
        }
    }

    globalThis.__onNodeInserted = _onNodeInserted;

    const _collectionIndex = (prop) => typeof prop === "string" && /^(0|[1-9]\d*)$/.test(prop);
    const _collectionState = new WeakMap();

    class NodeList {
        constructor(data, mode = 0) {
            const state = {
                source: typeof data === "function" ? data : null,
                mode,
                ids: [],
            };
            _collectionState.set(this, state);
            this._refresh(state.source ? undefined : data);
            if (!state.source) return;
            const proxy = new Proxy(this, {
                get(target, prop, receiver) {
                    if (prop === "length" || _collectionIndex(prop)) target._refresh();
                    return Reflect.get(target, prop, receiver);
                },
                ownKeys(target) { target._refresh(); return Reflect.ownKeys(target); },
                getOwnPropertyDescriptor(target, prop) {
                    if (_collectionIndex(prop)) target._refresh();
                    return Reflect.getOwnPropertyDescriptor(target, prop);
                },
            });
            _collectionState.set(proxy, state);
            return proxy;
        }
        _refresh(explicitData) {
            const state = _collectionState.get(this);
            if (!state || (explicitData === undefined && !state.source)) return;
            const data = explicitData === undefined && state.source ? state.source() : (explicitData || []);
            for (let i = 0; i < state.ids.length; i++) delete this[i];
            state.ids = [];
            if (state.mode === 1) {
                for (let i = 0; i < data.length; i += 2) {
                    const id = data[i];
                    state.ids.push(id);
                    this[i / 2] = _wrapNodeWithType(id, data[i + 1]);
                }
            } else {
                state.ids = data.slice ? data.slice() : Array.from(data || []);
                for (let i = 0; i < state.ids.length; i++) {
                    this[i] = state.mode === 2
                        ? _wrapNodeWithType(state.ids[i], 1)
                        : _wrapNode(state.ids[i]);
                }
            }
        }
        get length() { this._refresh(); return (_collectionState.get(this)?.ids || []).length; }
        item(index) { this._refresh(); const ids = _collectionState.get(this)?.ids || []; return index < ids.length ? this[index] : null; }
        forEach(cb, thisArg) {
            this._refresh();
            const ids = _collectionState.get(this)?.ids || [];
            for (let i = 0; i < ids.length; i++) cb.call(thisArg, this[i], i, this);
        }
        entries() {
            const self = this;
            let i = 0;
            return { next() { return i < self.length ? { value: [i, self[i++]], done: false } : { done: true }; }, [Symbol.iterator]() { return this; } };
        }
        keys() {
            const self = this;
            let i = 0;
            return { next() { return i < self.length ? { value: i++, done: false } : { done: true }; }, [Symbol.iterator]() { return this; } };
        }
        values() { return this[Symbol.iterator](); }
        [Symbol.iterator]() {
            let i = 0;
            const self = this;
            return {
                next() {
                    if (i < self.length) return { value: self[i++], done: false };
                    return { value: undefined, done: true };
                },
                [Symbol.iterator]() { return this; }
            };
        }
    }

    class HTMLCollection {
        constructor(data, mode = 0) {
            const state = {
                source: typeof data === "function" ? data : null,
                mode,
                ids: [],
            };
            _collectionState.set(this, state);
            this._refresh(state.source ? undefined : data);
            if (!state.source) return;
            const proxy = new Proxy(this, {
                get(target, prop, receiver) {
                    if (prop === "length" || _collectionIndex(prop)) target._refresh();
                    return Reflect.get(target, prop, receiver);
                },
                ownKeys(target) { target._refresh(); return Reflect.ownKeys(target); },
                getOwnPropertyDescriptor(target, prop) {
                    if (_collectionIndex(prop)) target._refresh();
                    return Reflect.getOwnPropertyDescriptor(target, prop);
                },
            });
            _collectionState.set(proxy, state);
            return proxy;
        }
        _refresh(explicitData) {
            const state = _collectionState.get(this);
            if (!state || (explicitData === undefined && !state.source)) return;
            const data = explicitData === undefined && state.source ? state.source() : (explicitData || []);
            for (let i = 0; i < state.ids.length; i++) delete this[i];
            state.ids = [];
            if (state.mode === 1) {
                for (let i = 0; i < data.length; i += 2) {
                    const id = data[i];
                    state.ids.push(id);
                    this[i / 2] = _wrapNodeWithType(id, data[i + 1]);
                }
            } else {
                state.ids = data.slice ? data.slice() : Array.from(data || []);
                for (let i = 0; i < state.ids.length; i++) {
                    this[i] = state.mode === 2
                        ? _wrapNodeWithType(state.ids[i], 1)
                        : _wrapNode(state.ids[i]);
                }
            }
        }
        get length() { this._refresh(); return (_collectionState.get(this)?.ids || []).length; }
        item(index) { this._refresh(); const ids = _collectionState.get(this)?.ids || []; return index >= 0 && index < ids.length ? this[index] : null; }
        namedItem(name) {
            this._refresh();
            const ids = _collectionState.get(this)?.ids || [];
            const wanted = String(name);
            for (let i = 0; i < ids.length; i++) {
                const el = this[i];
                if (el && (el.id === wanted || el.getAttribute?.("name") === wanted)) return el;
            }
            return null;
        }
        [Symbol.iterator]() {
            let i = 0;
            const self = this;
            return {
                next() {
                    return i < self.length
                        ? { value: self[i++], done: false }
                        : { value: undefined, done: true };
                },
                [Symbol.iterator]() { return this; },
            };
        }
    }

    Object.defineProperty(NodeList.prototype, Symbol.toStringTag, {
        value: "NodeList",
        configurable: true,
    });
    Object.defineProperty(HTMLCollection.prototype, Symbol.toStringTag, {
        value: "HTMLCollection",
        configurable: true,
    });

    class DOMTokenList {
        #nodeId;
        constructor(nodeId) { this.#nodeId = nodeId; }
        add(cls) { ops.op_dom_class_list_add(this.#nodeId, cls); }
        remove(cls) { ops.op_dom_class_list_remove(this.#nodeId, cls); }
        toggle(cls) {
            if (this.contains(cls)) { this.remove(cls); return false; }
            this.add(cls); return true;
        }
        contains(cls) {
            const attr = ops.op_dom_get_attribute(this.#nodeId, "class");
            return attr ? attr.split(/\s+/).includes(cls) : false;
        }
        get value() { return ops.op_dom_get_attribute(this.#nodeId, "class") || ""; }
        get length() { return this.value.split(/\s+/).filter(Boolean).length; }
        toString() { return this.value; }
        item(index) {
            const tokens = this.value.split(/\s+/).filter(Boolean);
            return tokens[index] != null ? tokens[index] : null;
        }
        // Real Chrome DOMTokenList is iterable; iterating yields each token
        // string. Some scripts spread element.classList — without
        // Symbol.iterator we throw "non-iterable" while Chrome returns the
        // token array.
        [Symbol.iterator]() {
            const tokens = this.value.split(/\s+/).filter(Boolean);
            let i = 0;
            return {
                next() {
                    if (i < tokens.length) return { value: tokens[i++], done: false };
                    return { value: undefined, done: true };
                },
                [Symbol.iterator]() { return this; }
            };
        }
        entries() {
            const tokens = this.value.split(/\s+/).filter(Boolean);
            let i = 0;
            return {
                next() {
                    if (i < tokens.length) { const idx = i; return { value: [idx, tokens[i++]], done: false }; }
                    return { value: undefined, done: true };
                },
                [Symbol.iterator]() { return this; }
            };
        }
        keys() {
            const n = this.length;
            let i = 0;
            return {
                next() {
                    if (i < n) return { value: i++, done: false };
                    return { value: undefined, done: true };
                },
                [Symbol.iterator]() { return this; }
            };
        }
        values() { return this[Symbol.iterator](); }
        forEach(cb, thisArg) {
            const tokens = this.value.split(/\s+/).filter(Boolean);
            for (let i = 0; i < tokens.length; i++) {
                cb.call(thisArg, tokens[i], i, this);
            }
        }
    }

    // EventTarget is the base of the DOM prototype chain in real Chrome:
    //   EventTarget ← Node ← Element ← HTMLElement ← HTMLDivElement etc.
    // Some scripts check `document instanceof EventTarget === true`
    // and walk Object.getPrototypeOf chains expecting this layout.
    const EventTarget = globalThis.EventTarget || class EventTarget {
        constructor() {}
        addEventListener(type, listener, options) {}
        removeEventListener(type, listener, options) {}
        dispatchEvent(event) { return true; }
    };
    globalThis.EventTarget = EventTarget;

    class Node extends EventTarget {
        constructor(nodeId) {
            super();
            _nodeIds.set(this, nodeId);
        }
        // nodeType constants
        static ELEMENT_NODE = 1;
        static TEXT_NODE = 3;
        static COMMENT_NODE = 8;
        static DOCUMENT_NODE = 9;
        static DOCUMENT_FRAGMENT_NODE = 11;
        static DOCUMENT_TYPE_NODE = 10;
        static PROCESSING_INSTRUCTION_NODE = 7;
        static ATTRIBUTE_NODE = 2;
        static CDATA_SECTION_NODE = 4;

        get nodeType() {
            const t = _nodeTypes.get(this);
            return t !== undefined ? t : ops.op_dom_get_node_type(_getNodeId(this));
        }
        get nodeName() {
            const type = this.nodeType;
            if (type === 1) return this.tagName;
            if (type === 3) return "#text";
            if (type === 8) return "#comment";
            if (type === 9) return "#document";
            if (type === 11) return "#document-fragment";
            return "";
        }
        get nodeValue() {
            const type = this.nodeType;
            if (type === 3 || type === 8) return ops.op_dom_get_text_content(_getNodeId(this));
            return null;
        }
        set nodeValue(val) {
            const type = this.nodeType;
            if (type === 3 || type === 8) _setCharacterData(this, val);
        }
        get ownerDocument() {
            return this.nodeType === 9 ? null : _document;
        }
        get isConnected() { return ops.op_dom_is_connected(_getNodeId(this)); }
        get baseURI() {
            return globalThis.location?.href || "about:blank";
        }
        get parentNode() { return _wrapNode(ops.op_dom_get_parent(_getNodeId(this))); }
        get parentElement() {
            const p = this.parentNode;
            return p && p.nodeType === 1 ? p : null;
        }
        get childNodes() {
            const id = _getNodeId(this);
            return new NodeList(() => ops.op_dom_get_children_with_types(id), 1);
        }
        get firstChild() { return _wrapNode(ops.op_dom_get_first_child(_getNodeId(this))); }
        get lastChild() { return _wrapNode(ops.op_dom_get_last_child(_getNodeId(this))); }
        get nextSibling() { return _wrapNode(ops.op_dom_get_next_sibling(_getNodeId(this))); }
        get previousSibling() { return _wrapNode(ops.op_dom_get_prev_sibling(_getNodeId(this))); }
        get textContent() { return ops.op_dom_get_text_content(_getNodeId(this)); }
        set textContent(val) {
            if (_moObservers.length > 0) {
                const t = this.nodeType;
                if (t === 3 || t === 8) { _setCharacterData(this, val); return; }
            }
            ops.op_dom_set_text_content(_getNodeId(this), String(val));
        }
        appendChild(child) {
            // DOM Standard: appending a DocumentFragment inserts its children
            // in order and empties the fragment; the fragment node itself is
            // never attached. Real widgets commonly construct their iframe in
            // a fragment before committing it to the live document.
            if (child && child.nodeType === 11) {
                if (child === this) return child;
                while (child.firstChild) {
                    this.appendChild(child.firstChild);
                }
                return child;
            }
            try {
                if (child && child.isConnected && !this.isConnected && this.nodeType !== 9) {
                    _ceDisconnected(child);
                }
            } catch (_) {}
            ops.op_dom_append_child(_getNodeId(this), _getNodeId(child));
            _onNodeInserted(child);
            return child;
        }
        removeChild(child) {
            _ceDisconnected(child);
            ops.op_dom_remove_child(_getNodeId(this), _getNodeId(child));
            return child;
        }
        replaceChild(newChild, oldChild) {
            if (newChild && newChild.nodeType === 11) {
                if (newChild !== this) {
                    while (newChild.firstChild) {
                        this.insertBefore(newChild.firstChild, oldChild);
                    }
                }
                this.removeChild(oldChild);
                return oldChild;
            }
            const parent = _getNodeId(this);
            const oldId = _getNodeId(oldChild);
            const newId = _getNodeId(newChild);
            _ceDisconnected(oldChild);
            try {
                if (newChild && newChild.isConnected && !this.isConnected && this.nodeType !== 9) {
                    _ceDisconnected(newChild);
                }
            } catch (_) {}
            ops.op_dom_insert_before(parent, newId, oldId);
            ops.op_dom_remove_child(parent, oldId);
            _onNodeInserted(newChild);
            return oldChild;
        }
        insertBefore(newChild, refChild) {
            if (refChild === null || refChild === undefined) return this.appendChild(newChild);
            if (newChild && newChild.nodeType === 11) {
                if (newChild === this) return newChild;
                while (newChild.firstChild) {
                    this.insertBefore(newChild.firstChild, refChild);
                }
                return newChild;
            }
            try {
                if (newChild && newChild.isConnected && !this.isConnected && this.nodeType !== 9) {
                    _ceDisconnected(newChild);
                }
            } catch (_) {}
            ops.op_dom_insert_before(_getNodeId(this), _getNodeId(newChild), _getNodeId(refChild));
            _onNodeInserted(newChild);
            return newChild;
        }
        cloneNode(deep = false) {
            const newId = ops.op_dom_clone_node(_getNodeId(this), !!deep);
            return _wrapNode(newId);
        }
        contains(other) {
            if (!other || typeof other !== "object") return false;
            const oid = _getNodeId(other);
            if (oid === null || oid === undefined || oid < 0) return false;
            return ops.op_dom_contains(_getNodeId(this), oid);
        }
        hasChildNodes() { return ops.op_dom_get_children(_getNodeId(this)).length > 0; }
        getRootNode(options = {}) {
            let n = this;
            for (;;) {
                if (n.parentNode) {
                    n = n.parentNode;
                    continue;
                }
                if (options && options.composed && n instanceof ShadowRoot) {
                    n = n.host;
                    continue;
                }
                return n;
            }
        }
        normalize() {
            // Merge adjacent text nodes
            const children = ops.op_dom_get_children(_getNodeId(this));
            let prevTextId = null;
            for (const cid of children) {
                if (ops.op_dom_get_node_type(cid) === 3) {
                    if (prevTextId !== null) {
                        const prevText = ops.op_dom_get_text_content(prevTextId);
                        const curText = ops.op_dom_get_text_content(cid);
                        ops.op_dom_set_text_content(prevTextId, prevText + curText);
                        ops.op_dom_remove_child(_getNodeId(this), cid);
                    } else {
                        prevTextId = cid;
                    }
                } else {
                    prevTextId = null;
                }
            }
        }
        isEqualNode(other) {
            if (!other) return false;
            if (this === other) return true;
            if (this.nodeType !== other.nodeType) return false;
            if (this.nodeType === 1) return this.outerHTML === other.outerHTML;
            return this.textContent === other.textContent;
        }
        isSameNode(other) { return this === other; }
        compareDocumentPosition(other) {
            if (this === other) return 0;
            if (this.contains(other)) return 20; // DOCUMENT_POSITION_CONTAINED_BY | FOLLOWING
            if (other.contains(this)) return 10; // DOCUMENT_POSITION_CONTAINS | PRECEDING
            return 4; // DOCUMENT_POSITION_FOLLOWING
        }
    }

    // --- Internal Bridge ---
    if (!globalThis.__browser_oxide) {
        Object.defineProperty(globalThis, '__browser_oxide', { value: {}, enumerable: false, configurable: true });
    }
    globalThis.__browser_oxide._getNodeId = _getNodeId;
    globalThis.__browser_oxide._wrapNode = _wrapNode;
    globalThis.__browser_oxide._setCurrentScript = _setCurrentScript;

    function _createStyleProxy(nodeId) {
        const cache = {};
        const raw = ops.op_dom_get_attribute(nodeId, "style") || "";
        for (const part of raw.split(";")) {
            const idx = part.indexOf(":");
            if (idx > 0) cache[part.slice(0, idx).trim()] = part.slice(idx + 1).trim();
        }
        function flush() {
            const parts = [];
            for (const k in cache) { if (cache[k] !== "") parts.push(k + ": " + cache[k]); }
            ops.op_dom_set_attribute(nodeId, "style", parts.join("; "));
        }
        const toKebab = (p) => p.replace(/[A-Z]/g, m => "-" + m.toLowerCase());
        const style = Object.create(globalThis.CSSStyleDeclaration.prototype || Object.prototype);
        return new Proxy(style, {
            get(target, prop) {
                if (prop === "setProperty") return (name, value) => { cache[name] = String(value); flush(); };
                if (prop === "getPropertyValue") return (name) => cache[name] || "";
                if (prop === "removeProperty") return (name) => { const old = cache[name] || ""; delete cache[name]; flush(); return old; };
                if (prop === "cssText") return ops.op_dom_get_attribute(nodeId, "style") || "";
                if (prop === "length") return Object.keys(cache).length;
                if (prop === Symbol.toStringTag) return "CSSStyleDeclaration";
                if (typeof prop === "string") {
                    if (/^\d+$/.test(prop)) return Object.keys(cache)[parseInt(prop, 10)];
                    return cache[toKebab(prop)] || "";
                }
                return undefined;
            },
            set(target, prop, value) {
                if (prop === "cssText") {
                    for (const k in cache) delete cache[k];
                    for (const part of String(value).split(";")) {
                        const idx = part.indexOf(":");
                        if (idx > 0) cache[part.slice(0, idx).trim()] = part.slice(idx + 1).trim();
                    }
                    flush();
                    return true;
                }
                cache[toKebab(prop)] = String(value);
                flush();
                return true;
            },
            // V8 Proxy invariant: has/ownKeys/getOwnPropertyDescriptor must
            // agree. Without explicit traps V8 reconciles against the empty
            // target object on every `prop in style` / Object.keys(style)
            // call — hot work that fingerprint scripts hit per WebIDL property under test.
            has(target, prop) {
                if (prop === "setProperty" || prop === "getPropertyValue" ||
                    prop === "removeProperty" || prop === "cssText") return true;
                if (typeof prop === "string") return Object.prototype.hasOwnProperty.call(cache, toKebab(prop));
                return false;
            },
            ownKeys() {
                return Object.keys(cache);
            },
            getOwnPropertyDescriptor(target, prop) {
                if (typeof prop !== "string") return undefined;
                const key = toKebab(prop);
                if (Object.prototype.hasOwnProperty.call(cache, key)) {
                    return { value: cache[key], enumerable: true, configurable: true, writable: true };
                }
                return undefined;
            }
        });
    }

    class Element extends Node {
        get tagName() {
            const t = _tagNames.get(this);
            return t !== undefined ? t : ops.op_dom_get_tag_name(_getNodeId(this)).toUpperCase();
        }
        get localName() {
            const t = _localNames.get(this);
            return t !== undefined ? t : ops.op_dom_get_tag_name(_getNodeId(this));
        }
        get id() { return ops.op_dom_get_attribute(_getNodeId(this), "id") || ""; }
        set id(val) { ops.op_dom_set_attribute(_getNodeId(this), "id", String(val)); }
        get className() { return ops.op_dom_get_attribute(_getNodeId(this), "class") || ""; }
        set className(val) { ops.op_dom_set_attribute(_getNodeId(this), "class", String(val)); }
        // HTML attribute-backed properties (script.src, link.href, img.src, etc.)
        // `.src` reflects the IDL attribute, which real Chrome returns as an
        // ABSOLUTE URL (resolved against the document base) — not the raw
        // relative attribute. Returning the raw relative value is a parity gap
        // that breaks any script deriving paths from its own `.src`. Resolve
        // against the document base; fall back to the raw value if URL parsing
        // fails, and keep "" for an absent/empty attribute (Chrome parity).
        get src() {
            const _raw = this.getAttribute("src");
            if (!_raw) return "";
            try {
                const _base = (globalThis.location && globalThis.location.href)
                    || (globalThis.__browser_oxide && globalThis.__browser_oxide._baseUrl)
                    || undefined;
                return new URL(_raw, _base).href;
            } catch (_) {
                return _raw;
            }
        }
        set src(val) {
            const raw = String(val);
            // Write the reflected content attribute directly. Calling the
            // virtual `setAttribute` here would recurse through the iframe
            // subclass override installed below.
            ops.op_dom_set_attribute(_getNodeId(this), "src", raw);
            // A connected iframe navigates when its src IDL attribute changes.
            // Queue a new frame id immediately so contentWindow switches away
            // from the previous browsing context before the Rust driver fetches
            // and materializes the replacement document.
            try {
                if (String(this.localName || "").toLowerCase() === "iframe"
                    && this.isConnected
                    && !this.hasAttribute("srcdoc")
                    && raw
                    && raw !== "about:blank"
                    && !/^javascript:/i.test(raw)
                    && !/^data:/i.test(raw)) {
                    const sameIsolateParent = typeof this.__oxParentRealm === "number";
                    _debugFrameLifecycle({
                        phase:'src-set',nodeId:_getNodeId(this),src:raw,
                        connected:!!this.isConnected,parentRealm:this.__oxParentRealm,
                        sameIsolateParent,
                    });
                    if (!sameIsolateParent) {
                        const nodeId = _getNodeId(this);
                        const abs = new URL(raw, (globalThis.location && globalThis.location.href) || "about:blank").href;
                        let name = "";
                        try { name = this.getAttribute("name") || this.name || ""; } catch (_) {}

                        // An iframe is commonly appended as about:blank and
                        // receives its remote src afterwards. Retire the local
                        // realm but preserve its public WindowProxy generation;
                        // the remote FrameHandle becomes the new backend.
                        const state = _getIframeState(this);
                        if (state && state._realmId !== undefined) {
                            try { _disposeIframeRealm(this, true); } catch (_) {}
                        }
                        const oldId = globalThis.__frameIdForNode && globalThis.__frameIdForNode[nodeId];
                        if (oldId !== undefined) _closedFrameIds.add(oldId);
                        const fid = ops.op_frame_pending(nodeId, abs, name);
                        if (fid) {
                            _debugFrameLifecycle({phase:'pending-src',nodeId,fid,src:abs});
                            globalThis.__oxRegisterChildFrame(nodeId, fid, new URL(abs).origin);
                        }
                    } else {
                        // Nested same-isolate frames stay in their parent realm
                        // hierarchy instead of being attached to the top Rust
                        // frame tree.
                        const nodeId = _getNodeId(this);
                        try { globalThis.__oxUnregisterChildFrame && globalThis.__oxUnregisterChildFrame(nodeId); } catch (_) {}
                        try { _disposeIframeRealm(this, true); } catch (_) {}
                        try { _getIframeWindow(this); } catch (_) {}
                    }
                } else if (String(this.localName || "").toLowerCase() === "iframe"
                    && this.isConnected
                    && !this.hasAttribute("srcdoc")) {
                    const nodeId = _getNodeId(this);
                    try { globalThis.__oxUnregisterChildFrame && globalThis.__oxUnregisterChildFrame(nodeId); } catch (_) {}
                    try { _disposeIframeRealm(this, true); } catch (_) {}
                    try { _getIframeWindow(this); } catch (_) {}
                }
            } catch (_) {}
        }
        get href() { return this.getAttribute("href") || ""; }
        set href(val) { this.setAttribute("href", String(val)); }
        get type() { return this.getAttribute("type") || ""; }
        set type(val) { this.setAttribute("type", String(val)); }
        get rel() { return this.getAttribute("rel") || ""; }
        set rel(val) { this.setAttribute("rel", String(val)); }
        get async() { return this.hasAttribute("async"); }
        set async(val) { if (val) this.setAttribute("async", ""); else this.removeAttribute("async"); }
        get defer() { return this.hasAttribute("defer"); }
        set defer(val) { if (val) this.setAttribute("defer", ""); else this.removeAttribute("defer"); }
        get crossOrigin() { return this.getAttribute("crossorigin"); }
        set crossOrigin(val) { if (val != null) this.setAttribute("crossorigin", String(val)); else this.removeAttribute("crossorigin"); }
        get integrity() { return this.getAttribute("integrity") || ""; }
        set integrity(val) { this.setAttribute("integrity", String(val)); }
        get referrerPolicy() { return this.getAttribute("referrerpolicy") || ""; }
        set referrerPolicy(val) { this.setAttribute("referrerpolicy", String(val)); }
        get classList() { return new DOMTokenList(_getNodeId(this)); }
        get innerHTML() { return ops.op_dom_get_inner_html(_getNodeId(this)); }
        set innerHTML(val) {
            // Replacing a subtree destroys any child navigables rooted inside
            // it before the new markup is installed.
            try {
                const oldFrames = this.querySelectorAll ? this.querySelectorAll("iframe") : [];
                for (let i = 0; i < oldFrames.length; i++) _disposeIframeRealm(oldFrames[i]);
            } catch (_) {}
            ops.op_dom_set_inner_html(_getNodeId(this), String(val));
            try { _normalizeTemplateContents(this); } catch (_) {}
            try { _syncTopFrameRegistry(); } catch (_) {}
        }
        get outerHTML() { return ops.op_dom_get_outer_html(_getNodeId(this)); }
        get children() {
            const id = _getNodeId(this);
            return new HTMLCollection(() => ops.op_dom_get_child_elements_with_types(id), 1);
        }
        get firstElementChild() { return _wrapNode(ops.op_dom_get_first_element_child(_getNodeId(this))); }
        get lastElementChild() { return _wrapNode(ops.op_dom_get_last_element_child(_getNodeId(this))); }
        getAttribute(name) { return ops.op_dom_get_attribute(_getNodeId(this), name); }
        setAttribute(name, value) { ops.op_dom_set_attribute(_getNodeId(this), name, String(value)); }
        removeAttribute(name) { ops.op_dom_remove_attribute(_getNodeId(this), name); }
        hasAttribute(name) { return ops.op_dom_has_attribute(_getNodeId(this), name); }
        // Namespaced / Attr-node APIs. react-dom's commit phase calls
        // setAttributeNS("http://www.w3.org/1999/xlink", "xlink:type", v) and
        // its unmount path drains el.attributes via removeAttributeNode.
        // We key attributes by the full qualified name, so the NS variants
        // just delegate with the qname as-is (set and remove both use the
        // same qualified form in react-dom, keeping lookups consistent).
        getAttributeNS(_ns, qname) { return this.getAttribute(String(qname)); }
        setAttributeNS(_ns, qname, value) { this.setAttribute(String(qname), value); }
        removeAttributeNS(_ns, qname) { this.removeAttribute(String(qname)); }
        hasAttributeNS(_ns, qname) { return this.hasAttribute(String(qname)); }
        setAttributeNode(attr) {
            if (attr && typeof attr.name === "string") {
                this.setAttribute(attr.name, attr.value);
                return attr;
            }
            return null;
        }
        removeAttributeNode(attr) {
            const name = attr && typeof attr.name === "string" ? attr.name : null;
            if (name === null || !this.hasAttribute(name)) return null;
            this.removeAttribute(name);
            return attr;
        }
        querySelector(sel) {
            const id = ops.op_dom_query_selector(_getNodeId(this), sel);
            return id !== null ? _wrapNode(id) : null;
        }
        querySelectorAll(sel) {
            return new NodeList(ops.op_dom_query_selector_all(_getNodeId(this), sel), 2);
        }
        matches(sel) { return ops.op_dom_matches(_getNodeId(this), String(sel)); }
        closest(sel) {
            const id = ops.op_dom_closest(_getNodeId(this), String(sel));
            return id !== -1 ? _wrapNode(id) : null;
        }
        getElementsByTagName(tag) {
            const id = _getNodeId(this);
            const name = String(tag);
            return new HTMLCollection(() => ops.op_dom_get_elements_by_tag_name(id, name), 2);
        }
        getElementsByClassName(cls) {
            const id = _getNodeId(this);
            const name = String(cls);
            return new HTMLCollection(() => ops.op_dom_get_elements_by_class_name(id, name), 2);
        }
        // Layout APIs (wired to taffy via layout_ext ops)
        getBoundingClientRect() {
            const r = ops.op_layout_get_bounding_rect(_getNodeId(this));
            return new DOMRect(r.x, r.y, r.width, r.height);
        }
        getClientRects() { return [this.getBoundingClientRect()]; }
        get offsetWidth() { return ops.op_layout_get_offset_width(_getNodeId(this)); }
        get offsetHeight() { return ops.op_layout_get_offset_height(_getNodeId(this)); }
        get offsetTop() { return ops.op_layout_get_offset_top(_getNodeId(this)); }
        get offsetLeft() { return ops.op_layout_get_offset_left(_getNodeId(this)); }
        get clientWidth() { return this.offsetWidth; }
        get clientHeight() { return this.offsetHeight; }
        get scrollWidth() { return this.offsetWidth; }
        get scrollHeight() { return this.offsetHeight; }
        get scrollTop() {
            const s = _scrollState.get(_getNodeId(this));
            return s ? s.top : 0;
        }
        set scrollTop(v) {
            const id = _getNodeId(this);
            const n = Number(v);
            const top = Number.isFinite(n) ? n : 0;
            const cur = _scrollState.get(id);
            if (cur) cur.top = top; else _scrollState.set(id, { top, left: 0 });
        }
        get scrollLeft() {
            const s = _scrollState.get(_getNodeId(this));
            return s ? s.left : 0;
        }
        set scrollLeft(v) {
            const id = _getNodeId(this);
            const n = Number(v);
            const left = Number.isFinite(n) ? n : 0;
            const cur = _scrollState.get(id);
            if (cur) cur.left = left; else _scrollState.set(id, { top: 0, left });
        }
        scrollIntoView(_arg) { /* spec no-op when no scrollable ancestor; safe stub */ }
        scrollTo(xOrOpts, y) {
            if (typeof xOrOpts === "object" && xOrOpts !== null) {
                if (xOrOpts.left !== undefined) this.scrollLeft = xOrOpts.left;
                if (xOrOpts.top !== undefined) this.scrollTop = xOrOpts.top;
            } else {
                this.scrollLeft = xOrOpts;
                this.scrollTop = y;
            }
        }
        scrollBy(xOrOpts, y) {
            if (typeof xOrOpts === "object" && xOrOpts !== null) {
                if (xOrOpts.left !== undefined) this.scrollLeft = this.scrollLeft + xOrOpts.left;
                if (xOrOpts.top !== undefined) this.scrollTop = this.scrollTop + xOrOpts.top;
            } else {
                this.scrollLeft = this.scrollLeft + xOrOpts;
                this.scrollTop = this.scrollTop + y;
            }
        }
        get offsetParent() { return this.parentElement; }
        // --- Modern DOM manipulation ---
        remove() {
            const parent = ops.op_dom_get_parent(_getNodeId(this));
            if (parent !== -1 && parent !== null) {
                ops.op_dom_remove_child(parent, _getNodeId(this));
            }
        }
        append(...nodes) {
            for (const node of nodes) {
                if (typeof node === "string") {
                    this.appendChild(_document.createTextNode(node));
                } else {
                    this.appendChild(node);
                }
            }
        }
        prepend(...nodes) {
            const first = this.firstChild;
            for (const node of nodes) {
                const n = typeof node === "string" ? _document.createTextNode(node) : node;
                if (first) {
                    this.insertBefore(n, first);
                } else {
                    this.appendChild(n);
                }
            }
        }
        after(...nodes) {
            const parent = this.parentNode;
            if (!parent) return;
            const next = this.nextSibling;
            for (const node of nodes) {
                const n = typeof node === "string" ? _document.createTextNode(node) : node;
                if (next) {
                    parent.insertBefore(n, next);
                } else {
                    parent.appendChild(n);
                }
            }
        }
        before(...nodes) {
            const parent = this.parentNode;
            if (!parent) return;
            for (const node of nodes) {
                const n = typeof node === "string" ? _document.createTextNode(node) : node;
                parent.insertBefore(n, this);
            }
        }
        replaceWith(...nodes) {
            const parent = this.parentNode;
            if (!parent) return;
            const next = this.nextSibling;
            this.remove();
            for (const node of nodes) {
                const n = typeof node === "string" ? _document.createTextNode(node) : node;
                if (next) {
                    parent.insertBefore(n, next);
                } else {
                    parent.appendChild(n);
                }
            }
        }
        replaceChildren(...nodes) {
            // Remove all existing children
            while (this.firstChild) this.removeChild(this.firstChild);
            this.append(...nodes);
        }
        // --- insertAdjacent family ---
        insertAdjacentHTML(position, html) {
            ops.op_dom_insert_adjacent_html(_getNodeId(this), position, html);
            try { _normalizeTemplateContents(this.parentNode || this); } catch (_) {}
            try { _syncTopFrameRegistry(); } catch (_) {}
        }
        insertAdjacentElement(position, element) {
            const parent = this.parentNode;
            switch (position) {
                case "beforebegin":
                    if (parent) parent.insertBefore(element, this);
                    break;
                case "afterbegin":
                    this.insertBefore(element, this.firstChild);
                    break;
                case "beforeend":
                    this.appendChild(element);
                    break;
                case "afterend":
                    if (parent) {
                        const next = this.nextSibling;
                        if (next) parent.insertBefore(element, next);
                        else parent.appendChild(element);
                    }
                    break;
            }
            return element;
        }
        insertAdjacentText(position, text) {
            const textNode = _document.createTextNode(text);
            this.insertAdjacentElement(position, textNode);
        }
        toggleAttribute(name, force) {
            if (force !== undefined) {
                if (force) { this.setAttribute(name, ""); return true; }
                else { this.removeAttribute(name); return false; }
            }
            if (this.hasAttribute(name)) { this.removeAttribute(name); return false; }
            this.setAttribute(name, ""); return true;
        }
        // --- Attribute helpers ---
        get attributes() {
            // NamedNodeMap-like object. Uses op_dom_get_attribute_names to
            // enumerate real attributes; previous shim hardcoded length: 0
            // which violates the V8 Proxy invariant ownKeys ⇔ has and made
            // per-element attribute audits do redundant work.
            const el = this;
            const id = _getNodeId(this);
            const namesOf = () => ops.op_dom_get_attribute_names(id);
            const itemFor = (name) => {
                const val = ops.op_dom_get_attribute(id, name);
                // typeof check (not truthiness): empty-string values
                // (e.g. <script async>) are real attributes and must surface.
                return typeof val === "string" ? { name, value: val, specified: true } : null;
            };
            return new Proxy([], {
                get(target, prop) {
                    // Real Chrome reports
                    // Object.prototype.toString.call(el.attributes) ===
                    // "[object NamedNodeMap]". The Proxy target is [], so
                    // without this it leaked "[object Array]", which differs
                    // from real Chrome. @@toStringTag (a string)
                    // overrides the array builtin tag per spec step 5.
                    if (prop === Symbol.toStringTag) return "NamedNodeMap";
                    if (prop === "length") return namesOf().length;
                    if (prop === "getNamedItem") return (name) => itemFor(String(name));
                    if (prop === "item") return (i) => {
                        const n = namesOf()[i];
                        return n ? itemFor(n) : null;
                    };
                    if (prop === Symbol.iterator) return function* () {
                        for (const n of namesOf()) yield itemFor(n);
                    };
                    if (typeof prop === "string" && /^\d+$/.test(prop)) {
                        const n = namesOf()[parseInt(prop, 10)];
                        return n ? itemFor(n) : undefined;
                    }
                    if (typeof prop === "string") return itemFor(prop);
                    return undefined;
                },
                has(target, prop) {
                    if (prop === "length" || prop === "getNamedItem" || prop === "item") return true;
                    if (typeof prop === "string" && /^\d+$/.test(prop)) {
                        return parseInt(prop, 10) < namesOf().length;
                    }
                    if (typeof prop === "string") return ops.op_dom_has_attribute(id, prop);
                    return false;
                },
                ownKeys() {
                    const names = namesOf();
                    const keys = [];
                    for (let i = 0; i < names.length; i++) keys.push(String(i));
                    return keys.concat(["length"]);
                },
                getOwnPropertyDescriptor(target, prop) {
                    if (prop === "length") {
                        return { value: namesOf().length, enumerable: false, configurable: false, writable: false };
                    }
                    if (typeof prop === "string" && /^\d+$/.test(prop)) {
                        const n = namesOf()[parseInt(prop, 10)];
                        if (n) return { value: itemFor(n), enumerable: true, configurable: true, writable: false };
                    }
                    return undefined;
                }
            });
        }
        get dataset() {
            const el = this;
            const id = _getNodeId(this);
            const toKebab = (p) => "data-" + p.replace(/[A-Z]/g, m => "-" + m.toLowerCase());
            const fromKebab = (a) => a.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
            const dataNames = () => ops.op_dom_get_attribute_names(id).filter(n => n.startsWith("data-"));
            return new Proxy({}, {
                get(target, prop) {
                    if (typeof prop !== "string") return undefined;
                    return ops.op_dom_get_attribute(id, toKebab(prop)) || undefined;
                },
                set(target, prop, value) {
                    el.setAttribute(toKebab(prop), String(value));
                    return true;
                },
                has(target, prop) {
                    if (typeof prop !== "string") return false;
                    return ops.op_dom_has_attribute(id, toKebab(prop));
                },
                deleteProperty(target, prop) {
                    if (typeof prop === "string") el.removeAttribute(toKebab(prop));
                    return true;
                },
                ownKeys() {
                    return dataNames().map(fromKebab);
                },
                getOwnPropertyDescriptor(target, prop) {
                    if (typeof prop !== "string") return undefined;
                    const attr = toKebab(prop);
                    if (ops.op_dom_has_attribute(id, attr)) {
                        return {
                            value: ops.op_dom_get_attribute(id, attr) || "",
                            enumerable: true, configurable: true, writable: true,
                        };
                    }
                    return undefined;
                }
            });
        }
        get nextElementSibling() { return _wrapNode(ops.op_dom_get_next_element_sibling(_getNodeId(this))); }
        get previousElementSibling() { return _wrapNode(ops.op_dom_get_prev_element_sibling(_getNodeId(this))); }
        get childElementCount() { return ops.op_dom_get_child_element_count(_getNodeId(this)); }
        // element.style — CSSStyleDeclaration proxy
        get style() {
            if (!this._style) this._style = _createStyleProxy(_getNodeId(this));
            return this._style;
        }
        // Interaction stubs
        click() { this.dispatchEvent(new Event("click", { bubbles: true })); }
        focus() { this.dispatchEvent(new Event("focus")); }
        blur() { this.dispatchEvent(new Event("blur")); }
        checkVisibility() { return true; }
        animate() { return { finished: Promise.resolve(), cancel() {}, play() {}, pause() {} }; }
        getAnimations() { return []; }
        attachShadow(init = {}) {
            const mode = init && init.mode;
            if (mode !== "open" && mode !== "closed") {
                throw new TypeError("Failed to execute 'attachShadow' on 'Element': mode must be 'open' or 'closed'.");
            }
            const hostName = String(this.localName || "").toLowerCase();
            const validStandardHosts = new Set([
                "article", "aside", "blockquote", "body", "div", "footer",
                "h1", "h2", "h3", "h4", "h5", "h6", "header", "main",
                "nav", "p", "section", "span",
            ]);
            if (!hostName.includes("-") && !validStandardHosts.has(hostName)) {
                throw new DOMException(
                    "Failed to execute 'attachShadow' on 'Element': This element does not support attachShadow.",
                    "NotSupportedError",
                );
            }
            if (_shadowRoots.has(this)) {
                throw new DOMException(
                    "Failed to execute 'attachShadow' on 'Element': Shadow root cannot be created on a host which already hosts a shadow tree.",
                    "NotSupportedError",
                );
            }
            const shadowId = ops.op_dom_attach_shadow(_getNodeId(this), mode);
            // Use _wrapNode — _wrap is not a defined helper. Was a stale
            // reference that threw `ReferenceError: _wrap is not defined`
            // whenever attachShadow was actually called — observable to
            // scripts that exercise Shadow DOM.
            const shadowRoot = _wrapNode(shadowId);
            Object.setPrototypeOf(shadowRoot, ShadowRoot.prototype);
            _shadowRoots.set(this, shadowRoot);
            _shadowHosts.set(shadowRoot, this);
            _shadowModes.set(shadowRoot, mode);
            try {
                const hostNodeId = _getNodeId(this);
                const rootNodeId = _getNodeId(shadowRoot);
                _shadowRootsByHostNode.set(hostNodeId, shadowRoot);
                _shadowHostNodeByRootNode.set(rootNodeId, hostNodeId);
                _shadowModeByRootNode.set(rootNodeId, mode);
            } catch (_) {}
            if (globalThis.__browser_oxide_debug) {
                try {
                    if (!Array.isArray(globalThis.__oxideShadowDebug)) {
                        globalThis.__oxideShadowDebug = [];
                    }
                    globalThis.__oxideShadowDebug.push(shadowRoot);
                } catch (_) {}
            }
            return shadowRoot;
        }
        get shadowRoot() {
            let root = _shadowRoots.get(this) || null;
            try {
                if (!root) root = _shadowRootsByHostNode.get(_getNodeId(this)) || null;
            } catch (_) {}
            let mode = root ? _shadowModes.get(root) : null;
            try {
                if (root && !mode) mode = _shadowModeByRootNode.get(_getNodeId(root)) || null;
            } catch (_) {}
            return root && mode === "open" ? root : null;
        }
    }

    // Full DOM prototype chain:
    //   EventTarget ← Node ← Element ← HTMLElement ← HTML*Element
    // Subclasses are mostly empty markers for instanceof checks. When an
    // element is created via _wrapNode, we do setPrototypeOf based on the
    // tag name to select the right specific class (HTMLDivElement etc.)
    // without having to create a dedicated Rust-side dispatch.
    class HTMLElement extends Element {}
    class HTMLDivElement extends HTMLElement {}
    class HTMLSpanElement extends HTMLElement {}
    class HTMLParagraphElement extends HTMLElement {}
    class HTMLHeadingElement extends HTMLElement {}
    class HTMLAnchorElement extends HTMLElement {}
    class HTMLImageElement extends HTMLElement {}
    Object.defineProperty(HTMLImageElement.prototype, "width", {
        get() {
            const attr = this.getAttribute("width");
            return attr ? parseInt(attr, 10) : 0;
        },
        enumerable: true, configurable: true
    });
    Object.defineProperty(HTMLImageElement.prototype, "height", {
        get() {
            const attr = this.getAttribute("height");
            return attr ? parseInt(attr, 10) : 0;
        },
        enumerable: true, configurable: true
    });
    Object.defineProperty(HTMLImageElement.prototype, "naturalWidth", {
        get() { return this.width; },
        enumerable: true, configurable: true
    });
    Object.defineProperty(HTMLImageElement.prototype, "naturalHeight", {
        get() { return this.height; },
        enumerable: true, configurable: true
    });
    Object.defineProperty(HTMLImageElement.prototype, "complete", {
        get() { return true; }, 
        enumerable: true, configurable: true
    });
    HTMLImageElement.prototype.decode = function() { return Promise.resolve(); };
    class HTMLInputElement extends HTMLElement {}
    class HTMLFormElement extends HTMLElement {
        submit() {
            const action = this.action || (globalThis.location ? globalThis.location.href : '');
            const method = (this.method || 'GET').toUpperCase();

            // Serialize form data
            const params = new URLSearchParams();
            const inputs = this.querySelectorAll('input, textarea, select');
            for (let i = 0; i < inputs.length; i++) {
                const el = inputs[i];
                const name = el.name;
                if (!name || el.disabled) continue;

                const type = (el.type || '').toLowerCase();
                if (type === 'submit' || type === 'button' || type === 'image') continue;
                if ((type === 'checkbox' || type === 'radio') && !el.checked) continue;

                params.append(name, el.value || '');
            }

            let finalUrl = action;
            let finalBody = null;

            if (method === 'GET') {
                const url = new URL(action, globalThis.location ? globalThis.location.href : 'about:blank');
                params.forEach((v, k) => url.searchParams.append(k, v));
                finalUrl = url.href;
            } else {
                finalBody = params.toString();
            }

            globalThis.__pendingNavigation = {
                url: finalUrl,
                method: method,
                body: finalBody,
                kind: 'assign'
            };
            // Signal the Rust event loop to short-circuit run_until_idle —
            // see crates/js_runtime/src/extensions/nav_ext.rs.
            try { ops.op_set_pending_nav(); } catch (_) {}
        }
        requestSubmit(submitter) {
            this.submit();
        }
    }

    // IDL property ↔ HTML attribute reflection. Scripts that configure form
    // fields via properties (el.name = 'x', form.action = url, form.method =
    // 'POST') expect the read-back to see what they set — which only works if
    // the property setter writes the underlying attribute. Without this,
    // programmatically-built forms look empty to our submit() serializer.
    // Universal primitive — matches HTML spec "reflect" behavior.
    const _reflectStr = (proto, prop, attr = prop, dflt = '') => {
        Object.defineProperty(proto, prop, {
            get() { const v = this.getAttribute(attr); return v == null ? dflt : v; },
            set(v) { this.setAttribute(attr, String(v)); },
            enumerable: true, configurable: true,
        });
    };
    const _reflectBool = (proto, prop, attr = prop) => {
        Object.defineProperty(proto, prop, {
            get() { return this.hasAttribute(attr); },
            set(v) {
                if (v) this.setAttribute(attr, '');
                else this.removeAttribute(attr);
            },
            enumerable: true, configurable: true,
        });
    };
    _reflectStr(HTMLInputElement.prototype, 'name');
    _reflectStr(HTMLInputElement.prototype, 'value');
    _reflectStr(HTMLInputElement.prototype, 'type', 'type', 'text');
    _reflectStr(HTMLInputElement.prototype, 'placeholder');
    _reflectBool(HTMLInputElement.prototype, 'checked');
    _reflectBool(HTMLInputElement.prototype, 'disabled');
    _reflectBool(HTMLInputElement.prototype, 'readOnly', 'readonly');
    _reflectBool(HTMLInputElement.prototype, 'required');
    _reflectStr(HTMLFormElement.prototype, 'action');
    _reflectStr(HTMLFormElement.prototype, 'method', 'method', 'get');
    _reflectStr(HTMLFormElement.prototype, 'enctype', 'enctype', 'application/x-www-form-urlencoded');
    _reflectStr(HTMLFormElement.prototype, 'target');
    _reflectStr(HTMLFormElement.prototype, 'name');
    _reflectBool(HTMLFormElement.prototype, 'noValidate', 'novalidate');

    // HTMLFormElement.prototype.elements — live HTMLFormControlsCollection
    // of the form's listed elements (HTML spec §6.4.3: button, fieldset,
    // input, object, output, select, textarea). Reddit's verify-page solver
    // calls `form.elements.namedItem('solution').value = token`; without
    // this getter that throws TypeError, the SPA's pendingNavigation is
    // never set, and the page returns iter=0 with the challenge stub.
    Object.defineProperty(HTMLFormElement.prototype, 'elements', {
        get() {
            const form = this;
            const controls = form.querySelectorAll(
                'button, fieldset, input, object, output, select, textarea',
            );
            const len = controls.length;
            const ctor = globalThis.HTMLFormControlsCollection;
            const wrap = ctor && ctor.prototype
                ? Object.create(ctor.prototype)
                : Object.create(null);
            for (let i = 0; i < len; i++) {
                Object.defineProperty(wrap, i, {
                    value: controls[i],
                    writable: false, configurable: true, enumerable: true,
                });
            }
            Object.defineProperty(wrap, 'length', {
                value: len,
                writable: false, configurable: true, enumerable: false,
            });
            Object.defineProperty(wrap, 'item', {
                value: function item(idx) {
                    idx = Math.trunc(+idx);
                    return idx >= 0 && idx < len ? wrap[idx] : null;
                },
                writable: true, configurable: true, enumerable: false,
            });
            Object.defineProperty(wrap, 'namedItem', {
                value: function namedItem(name) {
                    if (typeof name !== 'string' || name === '') return null;
                    const matches = [];
                    for (let i = 0; i < len; i++) {
                        const el = controls[i];
                        if (el.name === name || el.id === name) matches.push(el);
                    }
                    if (matches.length === 0) return null;
                    if (matches.length === 1) return matches[0];
                    // Spec: multiple → RadioNodeList. Returning an array
                    // covers reddit's single-name case + iteration.
                    return matches;
                },
                writable: true, configurable: true, enumerable: false,
            });
            Object.defineProperty(wrap, Symbol.iterator, {
                value: function* () {
                    for (let i = 0; i < len; i++) yield wrap[i];
                },
                writable: true, configurable: true, enumerable: false,
            });
            return wrap;
        },
        configurable: true,
        enumerable: true,
    });

    class HTMLButtonElement extends HTMLElement {}
    class HTMLSelectElement extends HTMLElement {}
    class HTMLTextAreaElement extends HTMLElement {}
    class HTMLCanvasElement extends HTMLElement {}
    Object.defineProperty(HTMLCanvasElement.prototype, "width", {
        get() {
            const attr = this.getAttribute("width");
            return attr ? parseInt(attr, 10) : 300;
        },
        set(v) { this.setAttribute("width", v); },
        enumerable: true, configurable: true
    });
    Object.defineProperty(HTMLCanvasElement.prototype, "height", {
        get() {
            const attr = this.getAttribute("height");
            return attr ? parseInt(attr, 10) : 150;
        },
        set(v) { this.setAttribute("height", v); },
        enumerable: true, configurable: true
    });
    HTMLCanvasElement.prototype.toDataURL = function(type, quality) {
        if (!this._canvasId) {
            let osName = "Linux", canvasSeed = 0n;
            try {
                if (ops.op_has_stealth_profile && ops.op_has_stealth_profile()) {
                    osName = ops.op_get_profile_value("os_name") || "Linux";
                    canvasSeed = BigInt(ops.op_get_profile_value("canvas_seed") || "0");
                }
            } catch (_e) { /* fall back to defaults */ }
            this._canvasId = ops.op_canvas_create(this.width, this.height, osName, canvasSeed);
        }
        return ops.op_canvas_to_data_url(this._canvasId);
    };
    class HTMLScriptElement extends HTMLElement {}
    class HTMLStyleElement extends HTMLElement {}
    class HTMLLinkElement extends HTMLElement {}
    class HTMLMetaElement extends HTMLElement {}
    _reflectStr(HTMLMetaElement.prototype, 'name');
    _reflectStr(HTMLMetaElement.prototype, 'content');
    _reflectStr(HTMLMetaElement.prototype, 'httpEquiv', 'http-equiv');
    _reflectStr(HTMLMetaElement.prototype, 'media');
    class HTMLTableElement extends HTMLElement {}
    class HTMLIFrameElement extends HTMLElement {}
    _reflectStr(HTMLIFrameElement.prototype, 'name');
    class HTMLVideoElement extends HTMLElement {}
    class HTMLAudioElement extends HTMLElement {}
    class HTMLBodyElement extends HTMLElement {}
    class HTMLHeadElement extends HTMLElement {}
    class HTMLHtmlElement extends HTMLElement {}
    class HTMLUListElement extends HTMLElement {}
    class HTMLOListElement extends HTMLElement {}
    class HTMLLIElement extends HTMLElement {}
    class HTMLTableRowElement extends HTMLElement {}
    class HTMLTableCellElement extends HTMLElement {}
    class HTMLTableSectionElement extends HTMLElement {}
    class HTMLLabelElement extends HTMLElement {}
    class HTMLOptionElement extends HTMLElement {}
    Object.defineProperty(HTMLOptionElement.prototype, 'value', {
        get() { return this.hasAttribute('value') ? this.getAttribute('value') : (this.textContent || '').trim(); },
        set(v) { this.setAttribute('value', String(v)); },
        enumerable: true, configurable: true,
    });
    Object.defineProperty(HTMLOptionElement.prototype, 'selected', {
        get() { return this.hasAttribute('selected'); },
        set(v) { if (v) this.setAttribute('selected', ''); else this.removeAttribute('selected'); },
        enumerable: true, configurable: true,
    });
    Object.defineProperty(HTMLOptionElement.prototype, 'text', {
        get() { return (this.textContent || '').trim(); },
        set(v) { this.textContent = String(v); },
        enumerable: true, configurable: true,
    });
    _reflectBool(HTMLOptionElement.prototype, 'defaultSelected', 'selected');
    _reflectBool(HTMLOptionElement.prototype, 'disabled');
    _reflectStr(HTMLOptionElement.prototype, 'label');

    // HTMLSelectElement.options — live HTMLOptionsCollection of the <option>s.
    // Code reading `select.options.length` throws without it.
    Object.defineProperty(HTMLSelectElement.prototype, 'options', {
        get() {
            const opts = this.querySelectorAll('option');
            const len = opts.length;
            const ctor = globalThis.HTMLOptionsCollection;
            const wrap = ctor && ctor.prototype ? Object.create(ctor.prototype) : Object.create(null);
            for (let i = 0; i < len; i++) {
                Object.defineProperty(wrap, i, {
                    value: opts[i], writable: false, configurable: true, enumerable: true,
                });
            }
            Object.defineProperty(wrap, 'length', {
                value: len, writable: false, configurable: true, enumerable: false,
            });
            Object.defineProperty(wrap, 'item', {
                value: function item(i) { i = Math.trunc(+i); return i >= 0 && i < len ? wrap[i] : null; },
                writable: true, configurable: true, enumerable: false,
            });
            Object.defineProperty(wrap, 'namedItem', {
                value: function namedItem(n) {
                    for (let i = 0; i < len; i++) { const o = opts[i]; if (o.id === n || o.name === n) return o; }
                    return null;
                },
                writable: true, configurable: true, enumerable: false,
            });
            Object.defineProperty(wrap, Symbol.iterator, {
                value: function* () { for (let i = 0; i < len; i++) yield wrap[i]; },
                writable: true, configurable: true, enumerable: false,
            });
            return wrap;
        },
        configurable: true, enumerable: true,
    });
    Object.defineProperty(HTMLSelectElement.prototype, 'selectedOptions', {
        get() { const o = this.options; const r = []; for (let i = 0; i < o.length; i++) if (o[i].selected) r.push(o[i]); return r; },
        configurable: true, enumerable: true,
    });
    Object.defineProperty(HTMLSelectElement.prototype, 'selectedIndex', {
        get() { const o = this.options; for (let i = 0; i < o.length; i++) if (o[i].selected) return i; return -1; },
        set(v) { const o = this.options; v = Math.trunc(+v); for (let i = 0; i < o.length; i++) o[i].selected = (i === v); },
        enumerable: true, configurable: true,
    });
    Object.defineProperty(HTMLSelectElement.prototype, 'value', {
        get() { const o = this.options; for (let i = 0; i < o.length; i++) if (o[i].selected) return o[i].value; return ''; },
        set(v) {
            const o = this.options; v = String(v);
            for (let i = 0; i < o.length; i++) o[i].selected = (o[i].value === v);
        },
        enumerable: true, configurable: true,
    });
    Object.defineProperty(HTMLSelectElement.prototype, 'length', {
        get() { return this.options.length; },
        configurable: true, enumerable: true,
    });
    Object.defineProperty(HTMLSelectElement.prototype, 'type', {
        get() { return this.multiple ? 'select-multiple' : 'select-one'; },
        configurable: true, enumerable: true,
    });
    _reflectBool(HTMLSelectElement.prototype, 'multiple');
    _reflectBool(HTMLSelectElement.prototype, 'disabled');
    _reflectBool(HTMLSelectElement.prototype, 'required');
    _reflectStr(HTMLSelectElement.prototype, 'name');
    // Template contents live in an inert DocumentFragment rather than in the
    // template element's ordinary child list. Key by stable Rust NodeId so a
    // recreated JS wrapper still resolves to the same contents fragment.
    const _templateContentsByNode = new Map();
    function _ensureTemplateContent(template) {
        if (!template) return null;
        let nodeId = null;
        try { nodeId = _getNodeId(template); } catch (_) {}
        if (nodeId !== null && _templateContentsByNode.has(nodeId)) {
            return _templateContentsByNode.get(nodeId);
        }

        let fragment = null;
        try { fragment = _wrapNode(ops.op_dom_create_document_fragment()); } catch (_) {}
        if (!fragment) return null;

        // html5ever may already have put parser-created template children on
        // the element. Move them with the raw op so no iframe insertion/frame
        // hooks fire while the nodes become inert template contents.
        try {
            while (template.firstChild) {
                ops.op_dom_append_child(_getNodeId(fragment), _getNodeId(template.firstChild));
            }
        } catch (_) {}
        if (nodeId !== null) _templateContentsByNode.set(nodeId, fragment);
        return fragment;
    }
    function _normalizeTemplateContents(root) {
        if (!root) return;
        const templates = [];
        try {
            if (root.nodeType === 1 && String(root.localName || '').toLowerCase() === 'template') {
                templates.push(root);
            }
        } catch (_) {}
        try {
            const ids = ops.op_dom_query_selector_all(_getNodeId(root), 'template') || [];
            for (let i = 0; i < ids.length; i++) {
                const template = _wrapNode(ids[i]);
                if (template) templates.push(template);
            }
        } catch (_) {}
        for (const template of templates) {
            const content = _ensureTemplateContent(template);
            if (content) _normalizeTemplateContents(content);
        }
    }
    class HTMLTemplateElement extends HTMLElement {
        get content() { return _ensureTemplateContent(this); }
        get innerHTML() {
            const content = _ensureTemplateContent(this);
            return content ? ops.op_dom_get_inner_html(_getNodeId(content)) : '';
        }
        set innerHTML(value) {
            const content = _ensureTemplateContent(this);
            if (!content) return;
            // Parsing into the detached fragment keeps latent iframes inert.
            ops.op_dom_set_inner_html(_getNodeId(content), String(value));
            _normalizeTemplateContents(content);
        }
        cloneNode(deep = false) {
            const clone = _wrapNode(ops.op_dom_clone_node(_getNodeId(this), false));
            try { _retargetElementProto(clone); } catch (_) {}
            if (deep && clone) clone.innerHTML = this.innerHTML;
            return clone;
        }
    }
    class HTMLPreElement extends HTMLElement {}
    class HTMLQuoteElement extends HTMLElement {}

    // Tag → specific HTML*Element prototype map. Anything not listed falls
    // back to HTMLElement.prototype.
    const _tagToProto = {
        div: HTMLDivElement.prototype,
        span: HTMLSpanElement.prototype,
        p: HTMLParagraphElement.prototype,
        h1: HTMLHeadingElement.prototype,
        h2: HTMLHeadingElement.prototype,
        h3: HTMLHeadingElement.prototype,
        h4: HTMLHeadingElement.prototype,
        h5: HTMLHeadingElement.prototype,
        h6: HTMLHeadingElement.prototype,
        a: HTMLAnchorElement.prototype,
        img: HTMLImageElement.prototype,
        input: HTMLInputElement.prototype,
        form: HTMLFormElement.prototype,
        button: HTMLButtonElement.prototype,
        select: HTMLSelectElement.prototype,
        textarea: HTMLTextAreaElement.prototype,
        canvas: HTMLCanvasElement.prototype,
        script: HTMLScriptElement.prototype,
        style: HTMLStyleElement.prototype,
        link: HTMLLinkElement.prototype,
        meta: HTMLMetaElement.prototype,
        table: HTMLTableElement.prototype,
        iframe: HTMLIFrameElement.prototype,
        video: HTMLVideoElement.prototype,
        audio: HTMLAudioElement.prototype,
        body: HTMLBodyElement.prototype,
        head: HTMLHeadElement.prototype,
        html: HTMLHtmlElement.prototype,
        ul: HTMLUListElement.prototype,
        ol: HTMLOListElement.prototype,
        li: HTMLLIElement.prototype,
        tr: HTMLTableRowElement.prototype,
        td: HTMLTableCellElement.prototype,
        th: HTMLTableCellElement.prototype,
        thead: HTMLTableSectionElement.prototype,
        tbody: HTMLTableSectionElement.prototype,
        tfoot: HTMLTableSectionElement.prototype,
        label: HTMLLabelElement.prototype,
        option: HTMLOptionElement.prototype,
        template: HTMLTemplateElement.prototype,
        pre: HTMLPreElement.prototype,
        blockquote: HTMLQuoteElement.prototype,
        q: HTMLQuoteElement.prototype,
    };

    // Adjust an Element instance's prototype to the tag-specific subclass
    // so `el instanceof HTMLDivElement` works as in real Chrome.
    function _retargetElementProto(el) {
        try {
            const tag = ops.op_dom_get_tag_name(_getNodeId(el)).toLowerCase();
            _localNames.set(el, tag);
            _tagNames.set(el, tag.toUpperCase());
            const proto = _tagToProto[tag] || HTMLElement.prototype;
            Object.setPrototypeOf(el, proto);
        } catch {}
    }

    class Text extends Node {
        get data() { return ops.op_dom_get_text_content(_getNodeId(this)); }
        set data(val) { _setCharacterData(this, val); }
        get length() { return this.data.length; }
        get wholeText() { return this.data; }
    }

    class Comment extends Node {
        get data() { return ops.op_dom_get_text_content(_getNodeId(this)); }
        set data(val) { _setCharacterData(this, val); }
    }

    class DocumentFragment extends Node {
        constructor(nodeId) {
            const id = nodeId === undefined ? ops.op_dom_create_document_fragment() : nodeId;
            super(id);
            _nodeTypes.set(this, 11);
            if (nodeId === undefined) _nodeCache.set(id, new WeakRef(this));
        }
        _scopedElementIds(selector) {
            const wanted = selector === undefined ? null : String(selector);
            const result = [];
            const stack = [];
            const roots = this.childNodes;
            for (let i = roots.length - 1; i >= 0; i--) stack.push(roots[i]);
            while (stack.length) {
                const node = stack.pop();
                if (!node) continue;
                if (node.nodeType === 1) {
                    if (wanted === null || node.matches(wanted)) result.push(_getNodeId(node));
                }
                const children = node.childNodes;
                if (children) {
                    for (let i = children.length - 1; i >= 0; i--) stack.push(children[i]);
                }
            }
            return result;
        }
        get children() {
            const id = _getNodeId(this);
            return new HTMLCollection(() => ops.op_dom_get_child_elements_with_types(id), 1);
        }
        get firstElementChild() { return _wrapNode(ops.op_dom_get_first_element_child(_getNodeId(this))); }
        get lastElementChild() { return _wrapNode(ops.op_dom_get_last_element_child(_getNodeId(this))); }
        get childElementCount() { return ops.op_dom_get_child_element_count(_getNodeId(this)); }
        querySelector(sel) {
            const ids = this._scopedElementIds(sel);
            return ids.length ? _wrapNode(ids[0]) : null;
        }
        querySelectorAll(sel) {
            return new NodeList(this._scopedElementIds(sel), 2);
        }
        getElementById(id) {
            const wanted = String(id);
            const ids = this._scopedElementIds();
            for (const nodeId of ids) {
                const el = _wrapNode(nodeId);
                if (el && el.id === wanted) return el;
            }
            return null;
        }
    }
    class ShadowRoot extends DocumentFragment {
        constructor() {
            throw new TypeError("Illegal constructor");
        }
        get host() {
            let host = _shadowHosts.get(this) || null;
            try {
                if (!host) {
                    const hostNodeId = _shadowHostNodeByRootNode.get(_getNodeId(this));
                    if (hostNodeId !== undefined) host = _wrapNode(hostNodeId);
                }
            } catch (_) {}
            return host;
        }
        get mode() {
            let mode = _shadowModes.get(this) || null;
            try {
                if (!mode) mode = _shadowModeByRootNode.get(_getNodeId(this)) || null;
            } catch (_) {}
            return mode || "open";
        }
        get adoptedStyleSheets() {
            return this._oxAdopted || (this._oxAdopted = []);
        }
        set adoptedStyleSheets(v) {
            const list = Array.prototype.slice.call(v || []).filter(Boolean);
            this._oxAdopted = list;
            this._oxAdoptedEls = this._oxAdoptedEls || new Map();
            for (const sheet of list) {
                let el = this._oxAdoptedEls.get(sheet);
                if (!el || !el.parentNode) {
                    el = this.ownerDocument
                        ? this.ownerDocument.createElement("style")
                        : document.createElement("style");
                    el.setAttribute("data-ox-adopted", "");
                    this.appendChild(el);
                    this._oxAdoptedEls.set(sheet, el);
                    if (!sheet._roots) sheet._roots = [];
                    if (!sheet._roots.includes(el)) sheet._roots.push(el);
                }
                if (typeof sheet._sync === "function") sheet._sync();
            }
        }
        get innerHTML() { return ops.op_dom_get_inner_html(_getNodeId(this)); }
        set innerHTML(html) {
            // Replacing a connected shadow subtree must destroy any previous
            // child navigables, including remote frame-tree mappings.
            try {
                const oldFrames = this.querySelectorAll("iframe");
                for (let i = 0; i < oldFrames.length; i++) {
                    const frame = oldFrames[i];
                    const nodeId = _getNodeId(frame);
                    try {
                        if (globalThis.__oxUnregisterChildFrame) {
                            globalThis.__oxUnregisterChildFrame(nodeId);
                        }
                    } catch (_) {}
                    try { _disposeIframeRealm(frame); } catch (_) {}
                }
            } catch (_) {}

            ops.op_dom_set_inner_html(_getNodeId(this), String(html));
            try { _normalizeTemplateContents(this); } catch (_) {}

            // Markup parsing bypasses Node.appendChild, so explicitly run the
            // iframe insertion hook for each new document-connected shadow
            // descendant. The hook itself rejects disconnected/template frames.
            try {
                const frames = this.querySelectorAll("iframe");
                for (let i = 0; i < frames.length; i++) _onNodeInserted(frames[i]);
            } catch (_) {}
        }
    }

    let _currentScript = null;
    // Sticky: keep the last executed <script> visible after its top-level
    // returns. Next.js Turbopack's async `registerChunk` runs module
    // factories in a promise continuation, where the app entry
    // (`getAssetPrefix`) still reads `document.currentScript`; Chrome keeps
    // the element readable there. A plain `null` clear would turn that read
    // into an `InvariantError` and sever the hydration bootstrap. The value
    // is overwritten by the next `set_current_script(Some)` and reset by the
    // per-navigation bootstrap re-run.
    function _setCurrentScript(el) { if (el !== null) _currentScript = el; }

    class HTMLAllCollection {
        constructor(doc) {
            this._doc = doc;
        }
        get length() { return this._doc.querySelectorAll("*").length; }
        item(i) { return this._doc.querySelectorAll("*")[i] || null; }
        namedItem(n) {
            return this._doc.getElementById(n) || 
                   this._doc.querySelector(`[name="${CSS.escape(n)}"]`) || 
                   null;
        }
        [Symbol.iterator]() {
            const nodes = this._doc.querySelectorAll("*");
            let i = 0;
            return {
                next() {
                    return i < nodes.length ? { value: nodes[i++], done: false } : { value: undefined, done: true };
                },
                [Symbol.iterator]() { return this; }
            };
        }
    }

    class Document extends Node {
        constructor(nodeId) {
            // Forward the document node id to Node so _getNodeId returns
            // the real Rust-side Document. Without this, document.nodeType
            // resolved to 0 (the "no such node" sentinel), which broke
            // anything walking parentNode→isConnected. Phase 7 follow-up.
            super(nodeId);
            if (!globalThis.__browser_oxide) {
                Object.defineProperty(globalThis, '__browser_oxide', { value: {}, enumerable: false, configurable: true });
            }
            // Capture initial base URL from ops or a global hint
            globalThis.__browser_oxide._baseUrl = ops.op_dom_get_base_url && ops.op_dom_get_base_url();

            const all = new HTMLAllCollection(this);
            // Hide 'all' from enumeration but keep it truthy
            Object.defineProperty(this, 'all', {
                get() { return all; },
                enumerable: false,
                configurable: true
            });
        }
        get scripts() { return this.getElementsByTagName("script"); }
        get currentScript() { return _currentScript; }
        get visibilityState() { return "visible"; }
        get hidden() { return false; }
        get webkitVisibilityState() { return "visible"; }
        get webkitHidden() { return false; }
        get fullscreenEnabled() { return true; }
        get webkitFullscreenEnabled() { return true; }
        get webkitIsFullScreen() { return false; }

        get documentElement() {
            const els = ops.op_dom_get_child_elements(ops.op_dom_document_node());
            return els.length > 0 ? _wrapNode(els[0]) : null;
        }
        get head() { return this.querySelector("head"); }
        get body() { return this.querySelector("body"); }
        get title() {
            const el = this.querySelector("title");
            return el ? el.textContent : "";
        }
        set title(val) {
            let el = this.querySelector("title");
            if (el) { el.textContent = val; }
        }
        getElementById(id) {
            const nodeId = ops.op_dom_get_element_by_id(id);
            return nodeId !== null ? _wrapNode(nodeId) : null;
        }
        getElementsByTagName(tag) {
            const id = ops.op_dom_document_node();
            const name = String(tag);
            return new HTMLCollection(() => ops.op_dom_get_elements_by_tag_name(id, name), 2);
        }
        getElementsByClassName(cls) {
            const id = ops.op_dom_document_node();
            return new HTMLCollection(() => ops.op_dom_get_elements_by_class_name(id, cls), 2);
        }
        // Used by Next.js route announcer & hash-scroll: look up by the
        // "name" content attribute. Static result is fine for both callers
        // (both index [0] immediately).
        getElementsByName(name) {
            const wanted = String(name);
            const els = this.querySelectorAll("*");
            const ids = [];
            for (let i = 0; i < els.length; i++) {
                const el = els[i];
                if (el && el.getAttribute && el.getAttribute("name") === wanted) ids.push(_getNodeId(el));
            }
            return new NodeList(ids, 2);
        }
        querySelector(sel) {
            const id = ops.op_dom_query_selector(ops.op_dom_document_node(), sel);
            return id !== null ? _wrapNode(id) : null;
        }
        querySelectorAll(sel) {
            return new NodeList(ops.op_dom_query_selector_all(ops.op_dom_document_node(), sel), 2);
        }
        createElement(tag) {
            const el = _wrapNode(ops.op_dom_create_element(tag));
            if (tag.toLowerCase() === "script") {
                let _src = "";
                // Capture the real descriptor to avoid infinite recursion
                const proto = Object.getPrototypeOf(el);
                const origSrc = Object.getOwnPropertyDescriptor(proto, 'src');

                Object.defineProperty(el, "src", {
                    get: () => _src,
                    set: (v) => {
                        _src = v;
                        if (v.includes("akam") || v.includes("ips.js") || v.includes("kpsdk")) {
                            console.log(`[DOM] dynamic script: ${v}`);
                        }
                        if (origSrc && origSrc.set) {
                            origSrc.set.call(el, v);
                        } else {
                            el.setAttribute("src", v);
                        }
                    },
                    configurable: true,
                });
            }
            return el;
        }
        createElementNS(ns, tag) {
            // For now, treat namespaced elements same as regular ones.
            return this.createElement(tag);
        }
        createTextNode(text) {
            return _wrapNode(ops.op_dom_create_text_node(text));
        }
        createDocumentFragment() {
            return _wrapNode(ops.op_dom_create_document_fragment());
        }
        createComment(text) {
            // Comment nodes have nodeType 8 in the DOM; use text node with special handling
            const id = ops.op_dom_create_text_node(""); // TODO: proper comment op
            return _wrapNode(id);
        }
        createEvent(type) {
            // Legacy event factory
            return new Event(type);
        }
        createRange() {
            return new Range();
        }
        createTreeWalker(root, whatToShow, filter) {
            return { currentNode: root, nextNode() { return null; }, previousNode() { return null; } };
        }
        createNodeIterator(root, whatToShow, filter) {
            return { nextNode() { return null; }, previousNode() { return null; } };
        }
        importNode(node, deep) { return node.cloneNode(deep); }
        adoptNode(node) {
            // Detach from current parent, adopt into this document
            if (node.parentNode) node.parentNode.removeChild(node);
            return node;
        }
        createAttribute(name) {
            return { name, value: "", specified: true };
        }
        // document.open/close — reset and finalize document stream
        open() { return this; }
        close() {}
        write(html) {
            // Document.write in Chrome synchronously executes any <script> tags
            // it inserts. Since op_dom_document_write returns the IDs of the
            // newly created nodes, we wrap them and trigger our insertion logic.
            const newIds = ops.op_dom_document_write(String(html));
            if (Array.isArray(newIds)) {
                for (const id of newIds) {
                    const node = _wrapNode(id);
                    if (node) _onNodeInserted(node, true); // Always sync for document.write
                }
            }
        }
        writeln(html) {
            this.write(html + "\n");
        }
        // Selection and editing
        execCommand(command, showUI, value) { return false; }
        queryCommandSupported(command) { return false; }
        queryCommandEnabled(command) { return false; }
        getSelection() { return globalThis.getSelection ? globalThis.getSelection() : null; }
        // Point-based queries. Per spec, a point OUTSIDE the viewport
        // (negative, or >= innerWidth/innerHeight) returns null / []. Real
        // Chrome returns null for elementFromPoint(-1,-1) and (99999,99999);
        // the previous unconditional `return this.body` differed from
        // real Chrome's layout behaviour for out-of-bounds points.
        // We lack full layout, so an in-viewport
        // point still approximates the topmost element with body (falling back
        // to documentElement) — but the viewport-bounds null result, which is
        // the detectable behaviour, is now spec-correct.
        _pointInViewport(x, y) {
            x = +x; y = +y;
            const w = globalThis.innerWidth || 0;
            const h = globalThis.innerHeight || 0;
            return x >= 0 && y >= 0 && x < w && y < h;
        }
        elementFromPoint(x, y) {
            if (!this._pointInViewport(x, y)) return null;
            return this.body || this.documentElement || null;
        }
        elementsFromPoint(x, y) {
            if (!this._pointInViewport(x, y)) return [];
            return this.body ? [this.body] : [];
        }
        caretPositionFromPoint(x, y) { return null; }
        hasFocus() { return true; }  // Anti-bot: must return true
        get readyState() { 
            return (globalThis._browser_oxide && globalThis._browser_oxide.__documentReadyState) || "complete"; 
        }
        get URL() { return globalThis.location?.href || "about:blank"; }
        get documentURI() { return this.URL; }
        get domain() { return globalThis.location?.hostname || ""; }
        get location() { return globalThis.location; }
        set location(val) { if (globalThis.location) globalThis.location.href = val; }
        get referrer() { return globalThis.__frameReferrer || ""; }
        get hidden() { return false; }
        get visibilityState() { return "visible"; }
        get cookie() {
            // Unified cookie jar: returns the mirror of net::cookies for this origin.
            // The mirror is refreshed synchronously on every page navigation and after
            // each fetch() response via _syncCookiesFromNet().
            if (!globalThis.__jsCookies) globalThis.__jsCookies = {};
            return Object.entries(globalThis.__jsCookies)
                .map(([k, v]) => `${k}=${v}`)
                .join("; ");
        }
        set cookie(val) {
            // Parse "name=value; path=/; ..." — update local mirror AND push to net::cookies.
            if (!globalThis.__jsCookies) globalThis.__jsCookies = {};
            const parts = String(val).split(";");
            const [name, ...rest] = (parts[0] || "").split("=");
            const key = name.trim();
            const value = rest.join("=").trim();
            if (!key) return;
            // Check for max-age=0 or expires in the past (delete cookie)
            const lower = String(val).toLowerCase();
            if (lower.includes("max-age=0") || lower.includes("max-age=-")) {
                delete globalThis.__jsCookies[key];
            } else {
                globalThis.__jsCookies[key] = value;
            }
            // Fire-and-forget propagation to the net layer.
            try {
                let url = globalThis.location?.href;
                if (!url || url === "about:blank" || url === "javascript:;" || url === "") {
                    url = globalThis.__browser_oxide && globalThis.__browser_oxide._baseUrl;
                }
                if (url) {
                    // Persist into the Rust
                    // jar SYNCHRONOUSLY. The async op_cookie_set was
                    // fire-and-forget, so a cookie set in the last microtasks
                    // before location.reload() (e.g. a challenge token) was
                    // lost — the reload re-fetched the stub. op_cookie_set_sync
                    // writes immediately (try_lock) with an async fallback.
                    if (ops.op_cookie_set_sync) {
                        ops.op_cookie_set_sync(url, String(val));
                    } else if (ops.op_cookie_set) {
                        ops.op_cookie_set(url, String(val));
                    }
                }
            } catch (e) { /* ignore */ }
        }
        // HTML legacy default per HTML Standard §2.4 — Chrome reports
        // "windows-1252" for HTML documents without an explicit
        // `<meta charset>` declaration. Verified against a real browser
        // (which reports "windows-1252").
        get characterSet() { return "windows-1252"; }
        get charset() { return "windows-1252"; }
        get contentType() { return "text/html"; }
        get compatMode() { return "CSS1Compat"; }
        // document.implementation — the DOMImplementation API. fpCollect and
        // several bot tests call createHTMLDocument() to verify the surface.
        get implementation() {
            return {
                createHTMLDocument(title) {
                    // Return a stub document with just enough of the Document
                    // API to satisfy fingerprinters. Real browsers return a
                    // fully functional Document, but our stubs never read it.
                    return {
                        title: title || "",
                        body: { innerHTML: "", appendChild: () => {} },
                        head: { appendChild: () => {} },
                        documentElement: { innerHTML: "" },
                        createElement(tag) {
                            return { tagName: tag.toUpperCase(), innerHTML: "", appendChild: () => {} };
                        },
                        createTextNode(t) { return { nodeValue: t }; },
                        querySelector() { return null; },
                        querySelectorAll() { return []; },
                    };
                },
                createDocument(ns, qualifiedName, doctype) {
                    return this.createHTMLDocument("");
                },
                createDocumentType(qualifiedName, publicId, systemId) {
                    return { name: qualifiedName, publicId, systemId };
                },
                hasFeature() { return true; },
            };
        }
        get doctype() { return null; }
        get defaultView() { return globalThis; }
        get activeElement() { return this.body; }
        get scripts() { return this.getElementsByTagName("script"); }
        get forms() { return this.getElementsByTagName("form"); }
        get images() { return this.getElementsByTagName("img"); }
        get links() { return this.getElementsByTagName("a"); }
        get embeds() { return this.getElementsByTagName("embed"); }
        get anchors() { return this.querySelectorAll("a[name]"); }
        get styleSheets() {
            const count = ops.op_dom_get_stylesheet_count();
            const sheets = [];
            for (let i = 0; i < count; i++) {
                sheets.push(new CSSStyleSheet(i));
            }
            return sheets;
        }
        // Constructable Stylesheets: materialize each adopted sheet as a
        // <style> in <head> so the regular stylesheet pipeline applies it.
        get adoptedStyleSheets() {
            return this._oxAdopted || (this._oxAdopted = []);
        }
        set adoptedStyleSheets(v) {
            const list = Array.prototype.slice.call(v || []).filter(Boolean);
            this._oxAdopted = list;
            const head = this.head || this.documentElement;
            if (!head) return;
            this._oxAdoptedEls = this._oxAdoptedEls || new Map();
            for (const sheet of list) {
                let el = this._oxAdoptedEls.get(sheet);
                if (!el || !el.parentNode) {
                    el = this.createElement("style");
                    el.setAttribute("data-ox-adopted", "");
                    head.appendChild(el);
                    this._oxAdoptedEls.set(sheet, el);
                    if (!sheet._roots) sheet._roots = [];
                    if (!sheet._roots.includes(el)) sheet._roots.push(el);
                }
                if (typeof sheet._sync === "function") sheet._sync();
                else {
                    try { el.textContent = Array.prototype.slice.call(sheet.cssRules || []).join("\n"); } catch (_e) { /* ignore */ }
                }
            }
        }
        get fullscreenElement() { return null; }
        get pointerLockElement() { return null; }
        exitFullscreen() { return Promise.resolve(); }
        exitPointerLock() {}
    }

    // --- CSSOM ---
    class CSSStyleSheet {
        constructor(index) { this._index = index; }
        get type() { return "text/css"; }
        get disabled() { return false; }
        get ownerNode() { return null; }
        get parentStyleSheet() { return null; }
        get title() { return null; }
        get media() { return { length: 0, mediaText: "" }; }
        get cssRules() {
            const raw = ops.op_dom_get_stylesheet_rules(this._index);
            return raw.map(r => new CSSStyleRule(r));
        }
        get rules() { return this.cssRules; }
        insertRule(_rule, _index) { return 0; }
        deleteRule(_index) {}
    }

    class CSSStyleRule {
        constructor({ selector_text, css_text, rule_type }) {
            this.selectorText = selector_text;
            this.cssText = css_text;
            this.type = rule_type;
            // Parse declarations into style-like object
            const styleObj = {};
            const declMatch = css_text.match(/\{([^}]*)\}/);
            if (declMatch) {
                for (const part of declMatch[1].split(";")) {
                    const [prop, ...vals] = part.split(":");
                    if (prop && vals.length) {
                        const p = prop.trim();
                        const v = vals.join(":").trim();
                        styleObj[p] = v;
                        // Also set camelCase version
                        const camel = p.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
                        if (camel !== p) styleObj[camel] = v;
                    }
                }
            }
            this.style = styleObj;
        }
    }

    // --- Range (minimal) ---
    class Range {
        constructor() {
            this.startContainer = null; this.startOffset = 0;
            this.endContainer = null; this.endOffset = 0;
            this.collapsed = true; this.commonAncestorContainer = null;
        }
        setStart(node, offset) { this.startContainer = node; this.startOffset = offset; this.collapsed = false; }
        setEnd(node, offset) { this.endContainer = node; this.endOffset = offset; }
        collapse(toStart) { this.collapsed = true; }
        cloneRange() { return new Range(); }
        getBoundingClientRect() { return new DOMRect(); }
        getClientRects() { return []; }
        createContextualFragment(html) {
            const div = _document.createElement("div");
            div.innerHTML = html;
            const frag = _document.createDocumentFragment();
            while (div.firstChild) frag.appendChild(div.firstChild);
            return frag;
        }
        toString() { return ""; }
    }

    // --- Selection (minimal) ---
    class Selection {
        get anchorNode() { return null; }
        get anchorOffset() { return 0; }
        get focusNode() { return null; }
        get focusOffset() { return 0; }
        get isCollapsed() { return true; }
        get rangeCount() { return 0; }
        getRangeAt(i) { return new Range(); }
        addRange(range) {}
        removeRange(range) {}
        removeAllRanges() {}
        collapse(node, offset) {}
        toString() { return ""; }
    }
    const _selection = new Selection();

    // Create the global document
    const _document = new Document(ops.op_dom_document_node());
    _nodeCache.set(ops.op_dom_document_node(), new WeakRef(_document));
    // The native HTML parser has already produced the initial DOM tree. Move
    // any parser-created template descendants into their inert contents before
    // page scripts or the frame registry can observe them as live descendants.
    try { _normalizeTemplateContents(_document); } catch (_) {}

    // Set globals
    // Symbol.toStringTag on every DOM class — some scripts
    // check Object.prototype.toString.call(node) and expect the Chrome
    // WebIDL brand name like "[object HTMLDivElement]". Without these
    // tags every node shows as "[object Object]", which differs from
    // real Chrome.
    const _tag = (cls, name) => {
        try {
            Object.defineProperty(cls.prototype, Symbol.toStringTag, {
                value: name, configurable: true,
            });
        } catch {}
    };
    _tag(EventTarget, "EventTarget");
    _tag(Node, "Node");
    _tag(Element, "Element");
    _tag(HTMLElement, "HTMLElement");
    _tag(HTMLDivElement, "HTMLDivElement");
    _tag(HTMLSpanElement, "HTMLSpanElement");
    _tag(HTMLParagraphElement, "HTMLParagraphElement");
    _tag(HTMLHeadingElement, "HTMLHeadingElement");
    _tag(HTMLAnchorElement, "HTMLAnchorElement");
    _tag(HTMLImageElement, "HTMLImageElement");
    _tag(HTMLInputElement, "HTMLInputElement");
    _tag(HTMLFormElement, "HTMLFormElement");
    _tag(HTMLButtonElement, "HTMLButtonElement");
    _tag(HTMLSelectElement, "HTMLSelectElement");
    _tag(HTMLTextAreaElement, "HTMLTextAreaElement");
    _tag(HTMLCanvasElement, "HTMLCanvasElement");
    _tag(HTMLScriptElement, "HTMLScriptElement");
    _tag(HTMLStyleElement, "HTMLStyleElement");
    _tag(HTMLLinkElement, "HTMLLinkElement");
    _tag(HTMLMetaElement, "HTMLMetaElement");
    _tag(HTMLTableElement, "HTMLTableElement");
    _tag(HTMLIFrameElement, "HTMLIFrameElement");
    _tag(HTMLVideoElement, "HTMLVideoElement");
    _tag(HTMLAudioElement, "HTMLAudioElement");
    _tag(HTMLBodyElement, "HTMLBodyElement");
    _tag(HTMLHeadElement, "HTMLHeadElement");
    _tag(HTMLHtmlElement, "HTMLHtmlElement");
    _tag(HTMLUListElement, "HTMLUListElement");
    _tag(HTMLOListElement, "HTMLOListElement");
    _tag(HTMLLIElement, "HTMLLIElement");
    _tag(HTMLTableRowElement, "HTMLTableRowElement");
    _tag(HTMLTableCellElement, "HTMLTableCellElement");
    _tag(HTMLTableSectionElement, "HTMLTableSectionElement");
    _tag(HTMLLabelElement, "HTMLLabelElement");
    _tag(HTMLOptionElement, "HTMLOptionElement");
    _tag(HTMLTemplateElement, "HTMLTemplateElement");
    _tag(HTMLPreElement, "HTMLPreElement");
    _tag(HTMLQuoteElement, "HTMLQuoteElement");
    _tag(Text, "Text");
    _tag(Comment, "Comment");
    _tag(DocumentFragment, "DocumentFragment");
    // Chrome exposes document as HTMLDocument (which extends Document).
    _tag(Document, "HTMLDocument");
    _tag(NodeList, "NodeList");
    _tag(DOMTokenList, "DOMTokenList");

    // documentElement (HTMLHtmlElement) and body (HTMLBodyElement) layout
    // dimensions in standards mode are viewport-clipped, NOT full document.
    // Default Element getters return offsetWidth/Height = full document
    // (e.g. 1914 × 28638 on some sites) which differs from real Chrome.
    // Real Chrome returns innerWidth × innerHeight (1440 × 789 on a typical
    // macOS 1440x900 viewport).
    {
        const _viewportW = () => (globalThis.innerWidth | 0) || 1440;
        const _viewportH = () => (globalThis.innerHeight | 0) || 789;
        Object.defineProperty(HTMLHtmlElement.prototype, 'clientWidth',  { get() { return _viewportW(); }, configurable: true });
        Object.defineProperty(HTMLHtmlElement.prototype, 'clientHeight', { get() { return _viewportH(); }, configurable: true });
        // documentElement.scrollWidth/Height are still full content size,
        // so leave the inherited offset-based getters in place for those.
    }

    // GlobalEventHandlers on* IDL attributes. The WHATWG mixin lives on Window,
    // Document, and HTMLElement; frameworks feature-detect them via `'on*' in document`.
    const _globalEventHandlerNames = [
        "onabort", "onauxclick", "onbeforeinput", "onbeforetoggle", "onblur",
        "oncancel", "oncanplay", "oncanplaythrough", "onchange", "onclick",
        "onclose", "oncontextmenu", "oncopy", "oncuechange", "oncut", "ondblclick",
        "ondrag", "ondragend", "ondragenter", "ondragleave", "ondragover",
        "ondragstart", "ondrop", "ondurationchange", "onemptied", "onended",
        "onerror", "onfocus", "onformdata", "oninput", "oninvalid", "onkeydown",
        "onkeypress", "onkeyup", "onload", "onloadeddata", "onloadedmetadata",
        "onloadstart", "onmousedown", "onmouseenter", "onmouseleave", "onmousemove",
        "onmouseout", "onmouseover", "onmouseup", "onmousewheel", "onpaste",
        "onpause", "onplay", "onplaying", "onpointercancel", "onpointerdown",
        "onpointerenter", "onpointerleave", "onpointermove", "onpointerout",
        "onpointerover", "onpointerrawupdate", "onpointerup", "onprogress",
        "onratechange", "onreset", "onresize", "onscroll", "onscrollend",
        "onsecuritypolicyviolation", "onseeked", "onseeking", "onselect",
        "onselectionchange", "onselectstart", "onslotchange", "onstalled",
        "onsubmit", "onsuspend", "ontimeupdate", "ontoggle", "ontransitioncancel",
        "ontransitionend", "ontransitionrun", "ontransitionstart", "onvolumechange",
        "onwaiting", "onwebkitanimationend", "onwebkitanimationiteration",
        "onwebkitanimationstart", "onwebkittransitionend", "onwheel",
    ];
    for (const _gehProto of [HTMLElement.prototype, Document.prototype]) {
        for (const _geh of _globalEventHandlerNames) {
            if (!Object.getOwnPropertyDescriptor(_gehProto, _geh)) {
                Object.defineProperty(_gehProto, _geh, {
                    value: null, writable: true, configurable: true, enumerable: true,
                });
            }
        }
    }

    globalThis.document = _document;
    globalThis.Document = Document;
    globalThis.HTMLDocument = Document;
    globalThis.Node = Node;
    globalThis.Element = Element;
    // Expose the real HTMLElement subclasses — the prototype chain is
    // EventTarget ← Node ← Element ← HTMLElement ← HTML*Element so that
    // `el instanceof HTMLDivElement` etc. works as in real Chrome.
    globalThis.HTMLElement = HTMLElement;
    globalThis.HTMLDivElement = HTMLDivElement;
    globalThis.HTMLSpanElement = HTMLSpanElement;
    globalThis.HTMLParagraphElement = HTMLParagraphElement;
    globalThis.HTMLHeadingElement = HTMLHeadingElement;
    globalThis.HTMLAnchorElement = HTMLAnchorElement;
    globalThis.HTMLImageElement = HTMLImageElement;
    globalThis.HTMLInputElement = HTMLInputElement;
    globalThis.HTMLFormElement = HTMLFormElement;
    globalThis.HTMLButtonElement = HTMLButtonElement;
    globalThis.HTMLSelectElement = HTMLSelectElement;
    globalThis.HTMLTextAreaElement = HTMLTextAreaElement;
    globalThis.HTMLCanvasElement = HTMLCanvasElement;
    globalThis.HTMLScriptElement = HTMLScriptElement;
    globalThis.HTMLStyleElement = HTMLStyleElement;
    globalThis.HTMLLinkElement = HTMLLinkElement;
    globalThis.HTMLMetaElement = HTMLMetaElement;
    globalThis.HTMLTableElement = HTMLTableElement;
    globalThis.HTMLIFrameElement = HTMLIFrameElement;
    globalThis.HTMLVideoElement = HTMLVideoElement;
    globalThis.HTMLAudioElement = HTMLAudioElement;
    globalThis.HTMLBodyElement = HTMLBodyElement;
    globalThis.HTMLHeadElement = HTMLHeadElement;
    globalThis.HTMLHtmlElement = HTMLHtmlElement;
    globalThis.HTMLUListElement = HTMLUListElement;
    globalThis.HTMLOListElement = HTMLOListElement;
    globalThis.HTMLLIElement = HTMLLIElement;
    globalThis.HTMLTableRowElement = HTMLTableRowElement;
    globalThis.HTMLTableCellElement = HTMLTableCellElement;
    globalThis.HTMLTableSectionElement = HTMLTableSectionElement;
    globalThis.HTMLLabelElement = HTMLLabelElement;
    globalThis.HTMLOptionElement = HTMLOptionElement;
    globalThis.HTMLTemplateElement = HTMLTemplateElement;
    globalThis.HTMLPreElement = HTMLPreElement;
    globalThis.HTMLQuoteElement = HTMLQuoteElement;
    globalThis.SVGElement = Element;
    globalThis.Text = Text;
    globalThis.Comment = Comment;
    globalThis.DocumentFragment = DocumentFragment;
    globalThis.ShadowRoot = ShadowRoot;
    globalThis.Document = Document;
    globalThis.NodeList = NodeList;
    globalThis.HTMLCollection = HTMLCollection;
    globalThis.DOMTokenList = DOMTokenList;
    globalThis.DOMRect = DOMRect;
    globalThis.DOMRectReadOnly = DOMRect;
    globalThis.Range = Range;
    globalThis.Selection = Selection;
    globalThis.getSelection = function() { return _selection; };

    // Image constructor — new Image(width, height). Returns an
    // HTMLImageElement whose naturalWidth/naturalHeight/complete are
    // accessors defined on the prototype (getters; not writable).
    // Constructor return of an object is the caller's `new Image(...)`.
    globalThis.Image = function Image(width, height) {
        const el = _document.createElement("img");
        if (width !== undefined) el.setAttribute("width", String(width));
        if (height !== undefined) el.setAttribute("height", String(height));
        return el;
    };

    // DOMParser
    globalThis.DOMParser = class DOMParser {
        parseFromString(str, type) {
            // Returns a minimal document-like object
            const frag = _document.createElement("div");
            frag.innerHTML = str;
            return {
                documentElement: frag,
                body: frag,
                querySelector(sel) { return frag.querySelector(sel); },
                querySelectorAll(sel) { return frag.querySelectorAll(sel); },
                getElementById(id) { return frag.querySelector("#" + id); },
            };
        }
    };

    // --- MutationObserver (real implementation) ---
    const _moObservers = []; // { observer, target, options }

    class MutationRecord {
        constructor(type, target) {
            this.type = type;
            this.target = target;
            this.addedNodes = [];
            this.removedNodes = [];
            this.attributeName = null;
            this.oldValue = null;
            this.previousSibling = null;
            this.nextSibling = null;
        }
    }

    class MutationObserver {
        constructor(callback) {
            this._callback = callback;
            this._records = [];
            this._active = false;
            this._targets = new Map(); // nodeId → options
        }
        observe(target, options = {}) {
            const nodeId = _getNodeId(target);
            this._targets.set(nodeId, { target, options });
            this._active = true;
            _moObservers.push(this);
        }
        disconnect() {
            this._active = false;
            this._targets.clear();
            const idx = _moObservers.indexOf(this);
            if (idx !== -1) _moObservers.splice(idx, 1);
        }
        takeRecords() {
            const r = this._records.slice();
            this._records = [];
            return r;
        }
        _notify(record) {
            if (!this._active) return;
            this._records.push(record);
            // queueMicrotask, not Promise.resolve(): a page Promise polyfill may
            // itself be built on MutationObserver, which would deadlock delivery.
            if (this._records.length === 1) {
                queueMicrotask(() => {
                    if (!this._active) return;
                    const batch = this._records.slice();
                    this._records = [];
                    if (batch.length > 0) this._callback(batch, this);
                });
            }
        }
    }

    // Notify matching observers of a mutation
    function _notifyMO(type, targetNodeId, init) {
        for (const obs of _moObservers) {
            if (!obs._active) continue;
            // Check if this observer watches this target (or subtree ancestor)
            let matched = obs._targets.has(targetNodeId);
            if (!matched) {
                // Check subtree: walk ancestors
                for (const [watchedId, { options }] of obs._targets) {
                    if (options.subtree) {
                        // Walk up from targetNodeId to see if watchedId is ancestor
                        let nid = targetNodeId;
                        while (nid !== -1 && nid !== null) {
                            if (nid === watchedId) { matched = true; break; }
                            nid = ops.op_dom_get_parent(nid);
                        }
                    }
                    if (matched) break;
                }
            }
            if (!matched) continue;

            // Check options match
            const opts = obs._targets.get(targetNodeId)?.options ||
                         [...obs._targets.values()].find(v => v.options.subtree)?.options || {};
            if (type === "childList" && !opts.childList) continue;
            if (type === "attributes" && !opts.attributes) continue;
            if (type === "characterData" && !opts.characterData) continue;

            const record = new MutationRecord(type, init.target || null);
            if (init.addedNodes) record.addedNodes = init.addedNodes;
            if (init.removedNodes) record.removedNodes = init.removedNodes;
            if (init.attributeName) record.attributeName = init.attributeName;
            if ("oldValue" in init) record.oldValue = init.oldValue;
            obs._notify(record);
        }
    }

    // Text mutations must emit a characterData record: some Promise polyfills
    // use a MutationObserver on a text node as their microtask scheduler.
    function _anyObserverWantsCDOldValue() {
        for (const obs of _moObservers) {
            if (!obs._active) continue;
            for (const { options } of obs._targets.values()) {
                if (options.characterData && options.characterDataOldValue) return true;
            }
        }
        return false;
    }
    function _setCharacterData(node, val) {
        const id = _getNodeId(node);
        let old = null;
        if (_moObservers.length > 0 && _anyObserverWantsCDOldValue()) {
            old = ops.op_dom_get_text_content(id);
        }
        ops.op_dom_set_text_content(id, String(val));
        if (_moObservers.length > 0) {
            _notifyMO("characterData", id, { target: node, oldValue: old });
        }
    }

    // Custom element lifecycle helper
    function _ceConnected(el) {
        if (el && el._ceUpgraded && typeof el.connectedCallback === "function") {
            try { el.connectedCallback(); } catch (e) { console.error(e); }
        }
    }
    // Assigned by the iframe subsystem below. Declared here because DOM removal
    // methods are installed before iframe state is initialized, but execute only
    // after bootstrap has completed.
    let _disposeIframeRealm = function() {};
    function _teardownIframeElement(frame) {
        if (!frame) return;
        try {
            if (globalThis.__oxUnregisterChildFrame) {
                globalThis.__oxUnregisterChildFrame(_getNodeId(frame));
            }
        } catch (_) {}
        try { _disposeIframeRealm(frame); } catch (_) {}
    }
    function _disposeIframeSubtree(el) {
        if (!el) return;
        try {
            if (typeof HTMLIFrameElement !== "undefined" && el instanceof HTMLIFrameElement) {
                _teardownIframeElement(el);
            }
        } catch (_) {}
        try {
            const nested = el.querySelectorAll ? el.querySelectorAll("iframe") : [];
            for (let i = 0; i < nested.length; i++) _teardownIframeElement(nested[i]);
        } catch (_) {}
    }
    function _ceDisconnected(el) {
        let root = null;
        try { root = _shadowRoots.get(el) || null; } catch (_) {}
        if (root) {
            try {
                const children = root.childNodes;
                for (let i = 0; i < children.length; i++) _ceDisconnected(children[i]);
            } catch (_) {}
        }
        _disposeIframeSubtree(el);
        if (el && el._ceUpgraded && typeof el.disconnectedCallback === "function") {
            try { el.disconnectedCallback(); } catch (e) { console.error(e); }
        }
    }

    // Window frame registry: tracks appended iframes so window[0], window[1], etc.
    // work correctly. Some scripts access window.frames[0].navigator.webdriver
    // (which is window[0] since frames===window in our engine). Without this,
    // window[0] is undefined → TypeError "Cannot read properties of undefined
    // (reading 'webdriver')".
    const _appendedIframes = [];
    const _topFrameNamedSlots = new Set();
    function _syncTopFrameRegistry() {
        let frames = [];
        try { frames = document.querySelectorAll("iframe") || []; } catch (_) { frames = []; }
        const previousCount = _appendedIframes.length;
        const nextNamed = new Map();
        _appendedIframes.length = 0;
        for (let i = 0; i < frames.length; i++) {
            const frame = frames[i];
            // This sync walks the top document, so any stale same-isolate
            // ownership marker left by adoption/reinsertion must be cleared.
            try { delete frame.__oxParentRealm; } catch (_) {}
            _appendedIframes.push(frame);
            try {
                const name = String((frame.getAttribute && frame.getAttribute("name")) || frame.name || "");
                if (name && !/^(0|[1-9]\d*)$/.test(name) && !nextNamed.has(name)) {
                    nextNamed.set(name, frame);
                }
            } catch (_) {}
            try {
                Object.defineProperty(globalThis, String(i), {
                    get: function() { return _frameWindowFor(_appendedIframes[i]); },
                    configurable: true,
                    enumerable: false,
                });
            } catch (_) {}
        }
        for (let i = frames.length; i < previousCount; i++) {
            try { delete globalThis[String(i)]; } catch (_) {}
        }
        try {
            Object.defineProperty(globalThis, "length", {
                value: frames.length,
                configurable: true,
                writable: true,
            });
        } catch (_) {}
        for (const name of Array.from(_topFrameNamedSlots)) {
            if (nextNamed.has(name)) continue;
            try { delete globalThis[name]; } catch (_) {}
            _topFrameNamedSlots.delete(name);
        }
        for (const [name, frame] of nextNamed) {
            const owned = _topFrameNamedSlots.has(name);
            let available = owned;
            if (!available) {
                try { available = !(name in globalThis); } catch (_) { available = false; }
            }
            if (!available) continue;
            try {
                Object.defineProperty(globalThis, name, {
                    get: function() { return _frameWindowFor(frame); },
                    configurable: true,
                    enumerable: true,
                });
                _topFrameNamedSlots.add(name);
            } catch (_) {}
        }
        try { globalThis.__ifAppendCount = frames.length; } catch (_) {}
    }

    // Wrap DOM mutation methods to fire MO notifications
    const _origAppendChild = Node.prototype.appendChild;
    Node.prototype.appendChild = function(child) {
        const result = _origAppendChild.call(this, child);
        _syncTopFrameRegistry();
        if (_moObservers.length > 0) {
            _notifyMO("childList", _getNodeId(this), { target: this, addedNodes: [child] });
        }
        return result;
    };

    const _origRemoveChild = Node.prototype.removeChild;
    Node.prototype.removeChild = function(child) {
        const result = _origRemoveChild.call(this, child);
        _syncTopFrameRegistry();
        if (_moObservers.length > 0) {
            _notifyMO("childList", _getNodeId(this), { target: this, removedNodes: [child] });
        }
        return result;
    };

    const _origInsertBefore = Node.prototype.insertBefore;
    Node.prototype.insertBefore = function(newChild, refChild) {
        const result = _origInsertBefore.call(this, newChild, refChild);
        _syncTopFrameRegistry();
        if (_moObservers.length > 0) {
            _notifyMO("childList", _getNodeId(this), { target: this, addedNodes: [newChild] });
        }
        return result;
    };

    const _origSetAttribute = Element.prototype.setAttribute;
    Element.prototype.setAttribute = function(name, value) {
        const oldVal = this.getAttribute(name);
        _origSetAttribute.call(this, name, value);
        if (_moObservers.length > 0) {
            _notifyMO("attributes", _getNodeId(this), { target: this, attributeName: name });
        }
        // Custom element attributeChangedCallback
        if (this._ceUpgraded && typeof this.attributeChangedCallback === "function") {
            const observed = this.constructor.observedAttributes;
            if (Array.isArray(observed) && observed.includes(name)) {
                try { this.attributeChangedCallback(name, oldVal, value); } catch (e) { console.error(e); }
            }
        }
    };

    const _origRemoveAttribute = Element.prototype.removeAttribute;
    Element.prototype.removeAttribute = function(name) {
        const oldVal = this.getAttribute(name);
        _origRemoveAttribute.call(this, name);
        if (_moObservers.length > 0) {
            _notifyMO("attributes", _getNodeId(this), { target: this, attributeName: name });
        }
        // Custom element attributeChangedCallback
        if (this._ceUpgraded && typeof this.attributeChangedCallback === "function") {
            const observed = this.constructor.observedAttributes;
            if (Array.isArray(observed) && observed.includes(name)) {
                try { this.attributeChangedCallback(name, oldVal, null); } catch (e) { console.error(e); }
            }
        }
    };

    // Element.remove() also triggers childList on parent
    const _origRemove = Element.prototype.remove;
    Element.prototype.remove = function() {
        const parent = this.parentNode;
        _ceDisconnected(this);
        _origRemove.call(this);
        _syncTopFrameRegistry();
        if (_moObservers.length > 0 && parent) {
            _notifyMO("childList", _getNodeId(parent), { target: parent, removedNodes: [this] });
        }
    };

    // Parser-created iframes exist before mutation wrappers run. Seed the
    // top-level indexed properties from the actual DOM once at bootstrap.
    _syncTopFrameRegistry();

    globalThis.MutationObserver = MutationObserver;
    globalThis.MutationRecord = MutationRecord;

    // --- iframe support (contentWindow / contentDocument) ---
    //
    // Many scripts perform iframe-realm checks:
    // they create or find an <iframe>, access `.contentWindow`, then pull
    // native constructors (TextEncoder, Function, Array, ...) from the iframe
    // window to compare against the main window's versions. A mismatch
    // reveals monkey-patching; an `undefined` contentWindow reveals a headless
    // browser that doesn't support iframes.
    //
    // We install `contentWindow` and `contentDocument` as GETTERS on
    // HTMLIFrameElement.prototype so EVERY iframe — whether parsed from HTML
    // or created via document.createElement — returns a valid window-shaped
    // Proxy that falls through to globalThis for any unknown property. The
    // per-iframe state is cached in a WeakMap keyed by the element.

    const _iframeState = new WeakMap();
    // Wrapper identity is not stable: the same Rust DOM node can be wrapped
    // into a fresh JS object by a later query. Browsing-context state therefore
    // uses NodeId as the authoritative key and WeakMap only as a fast path.
    const _iframeStateByNode = new Map();
    const _sameIsolateRealmForNode = Object.create(null);
    const _realmDocumentById = new Map();
    const _realmFrameHosts = new Map();
    const _realmFrameSlotCount = new Map();
    const _realmFrameNamedSlots = new Map();
    const _realmRegistrySyncing = new Set();
    function _iframeNodeId(el) {
        try {
            const id = _getNodeId(el);
            return (typeof id === "number" && id >= 0) ? id : null;
        } catch (_) { return null; }
    }
    function _getIframeState(el) {
        let state = null;
        try { state = _iframeState.get(el) || null; } catch (_) {}
        if (state) return state;
        const nodeId = _iframeNodeId(el);
        if (nodeId === null) return null;
        state = _iframeStateByNode.get(nodeId) || null;
        if (state) {
            try { _iframeState.set(el, state); } catch (_) {}
        }
        return state;
    }
    function _setIframeState(el, state) {
        try { _iframeState.set(el, state); } catch (_) {}
        const nodeId = _iframeNodeId(el);
        if (nodeId !== null) {
            _iframeStateByNode.set(nodeId, state);
            if (state && state._realmId !== undefined) {
                _sameIsolateRealmForNode[nodeId] = state._realmId;
            }
        }
        return state;
    }
    function _deleteIframeState(el) {
        try { _iframeState.delete(el); } catch (_) {}
        const nodeId = _iframeNodeId(el);
        if (nodeId !== null) {
            _iframeStateByNode.delete(nodeId);
            delete _sameIsolateRealmForNode[nodeId];
        }
    }
    const _realmPublicWindow = new Map();
    const _realmRawWindow = new Map();
    // Rebound once the stable public WindowProxy layer is installed below.
    // `_getIframeWindow` is defined earlier in the bootstrap and uses this
    // indirection when creating MessageEvent.source.
    let _detachPublicWindowProxyFor = function(_el, _state) {};
    _disposeIframeRealm = function(el, preserveWindowProxy = false) {
        let state;
        try { state = _getIframeState(el); } catch (_) { state = null; }
        if (!preserveWindowProxy) {
            try { _detachPublicWindowProxyFor(el, state); } catch (_) {}
        }
        if (state && state._realmId !== undefined) {
            try { ops.op_dispose_child_realm(state._realmId, !!preserveWindowProxy); } catch (_) {}
            _realmPublicWindow.delete(state._realmId);
            _realmRawWindow.delete(state._realmId);
            _realmBaseUrl.delete(state._realmId);
            _realmDocumentById.delete(state._realmId);
            _realmFrameHosts.delete(state._realmId);
            _realmFrameSlotCount.delete(state._realmId);
            _realmFrameNamedSlots.delete(state._realmId);
        }
        _deleteIframeState(el);
    };

    // Build a mirror realm: fresh constructors that mimic the parent's shape
    // but are reference-distinct, so cross-realm probes like
    //   iframe.contentWindow.Navigator !== Navigator
    //   iframe.contentWindow.Navigator.prototype !== Navigator.prototype
    // hold true while own-property-names lists remain identical. Each
    // mirrored function carries _nativeTag so Function.prototype.toString
    // produces "function NAME() { [native code] }" cross-realm.
    const _MIRRORED_CONSTRUCTORS = [
        "Navigator", "Window", "Document", "HTMLDocument",
        "EventTarget", "Node", "Element", "HTMLElement",
        "HTMLDivElement", "HTMLSpanElement", "HTMLBodyElement",
        "HTMLAnchorElement", "HTMLImageElement", "HTMLInputElement",
        "HTMLFormElement", "HTMLButtonElement", "HTMLSelectElement",
        "HTMLTextAreaElement", "HTMLCanvasElement", "HTMLScriptElement",
        "HTMLIFrameElement", "Event", "CustomEvent", "MouseEvent",
        "KeyboardEvent", "MessageEvent", "Array", "Object", "Function",
        "String", "Number", "Boolean", "Promise", "Error", "TypeError",
        "RangeError", "Map", "Set", "WeakMap", "WeakSet", "Date",
        "RegExp", "Symbol",
    ];

    // Capture the native-tag Symbol from the parent realm. stealth_bootstrap.js
    // exposes it as globalThis._nativeTag. We capture explicitly so the
    // freshToString and _mkNativeFn don't accidentally see undefined when
    // bare-identifier scope chain is shadowed by the IIFE parameter.
    const _NATIVE_TAG_SYMBOL = globalThis._nativeTag || Symbol.for('__browser_oxide_native__');

    function _mkNativeFn(name) {
        const fn = function() {};
        try {
            Object.defineProperty(fn, "name", { value: name, configurable: true });
            Object.defineProperty(fn, _NATIVE_TAG_SYMBOL, { value: name, configurable: true });
            // Per-instance toString returning native shape — used when the
            // patched Function.prototype.toString is bypassed by direct
            // .toString() calls. Mirrors stealth_bootstrap's _maskFunction.
            const ts = function toString() { return "function " + name + "() { [native code] }"; };
            Object.defineProperty(ts, _NATIVE_TAG_SYMBOL, { value: "toString", configurable: true });
            Object.defineProperty(ts, "name", { value: "toString", configurable: true });
            Object.defineProperty(fn, "toString", { value: ts, configurable: true });
        } catch (_) {}
        return fn;
    }

    // Constructors where `new w.X(...)` is genuinely "Illegal constructor"
    // in real Chrome (DOM interfaces with no exposed constructor). Calls
    // to `new` on these throw `TypeError: Illegal constructor`.
    // Constructors NOT in this set are real callable types — for those we
    // delegate `new` to the parent realm's constructor via `Reflect.construct`
    // so e.g. `new iframe.contentWindow.Function("return 1")` returns a
    // function in the iframe realm, matching real Chrome. Some scripts use
    // `new w.Function(...)` to materialize a fresh-realm function; if we
    // throw where real Chrome succeeds, that differs from real Chrome.
    const _ILLEGAL_CONSTRUCTORS = new Set([
        "Navigator", "Window", "Document", "HTMLDocument",
        "Node", "Element", "HTMLElement",
        "HTMLDivElement", "HTMLSpanElement", "HTMLBodyElement",
        "HTMLAnchorElement", "HTMLImageElement", "HTMLInputElement",
        "HTMLFormElement", "HTMLButtonElement", "HTMLSelectElement",
        "HTMLTextAreaElement", "HTMLCanvasElement", "HTMLScriptElement",
        "HTMLIFrameElement",
    ]);

    function _mkMirroredConstructor(parentCtor, name, freshGrandparentProto) {
        // Fresh constructor function — different identity than parent's.
        // For DOM-interface types real Chrome throws on `new`; for genuine
        // callable types (Function/Array/Map/Date/Event/...) we delegate to
        // the parent constructor via Reflect.construct so the result lives
        // in our fresh realm (via fresh.prototype = freshProto below).
        const isIllegal = _ILLEGAL_CONSTRUCTORS.has(name);
        const fresh = isIllegal
            ? function() {
                throw new TypeError("Failed to construct '" + name + "': Illegal constructor");
            }
            : function(...args) {
                try {
                    return Reflect.construct(parentCtor, args, fresh);
                } catch (e) {
                    // Symbol() throws on `new`; re-throw with the parent's
                    // exact shape (don't reword) so feature-detection that
                    // catches "Symbol is not a constructor" still matches.
                    throw e;
                }
            };
        try {
            Object.defineProperty(fresh, "name", { value: name, configurable: true });
            Object.defineProperty(fresh, _NATIVE_TAG_SYMBOL, { value: name, configurable: true });
            const ts = function toString() { return "function " + name + "() { [native code] }"; };
            Object.defineProperty(ts, _NATIVE_TAG_SYMBOL, { value: "toString", configurable: true });
            Object.defineProperty(ts, "name", { value: "toString", configurable: true });
            Object.defineProperty(fresh, "toString", { value: ts, configurable: true });
        } catch (_) {}

        // Build a fresh prototype mirroring own-property-names of parent's prototype.
        // Each method/getter/setter is a fresh function with native toString shape.
        let parentProto = null;
        try { parentProto = parentCtor && parentCtor.prototype; } catch (_) {}
        // The fresh prototype's own __proto__ must point at the FRESH grandparent
        // prototype (built earlier in _buildRemoteRealm's topological pass),
        // NOT at the parent realm's grandparent. Crossing realms here makes
        // a prototype-chain walk traverse the parent realm's full chain on
        // top of the fresh chain, multiplying its work O(N) → O(N²+).
        const freshProto = Object.create(freshGrandparentProto || Object.prototype);

        if (parentProto) {
            const propNames = Object.getOwnPropertyNames(parentProto);
            for (const propName of propNames) {
                if (propName === "constructor") continue;
                let desc;
                try { desc = Object.getOwnPropertyDescriptor(parentProto, propName); } catch (_) { continue; }
                if (!desc) continue;
                const newDesc = {
                    configurable: desc.configurable !== false,
                    enumerable: !!desc.enumerable,
                };
                if (desc.get || desc.set) {
                    if (desc.get) newDesc.get = _mkNativeFn("get " + propName);
                    if (desc.set) newDesc.set = _mkNativeFn("set " + propName);
                } else {
                    newDesc.writable = desc.writable !== false;
                    if (typeof desc.value === "function") {
                        // Function-valued props: replace with our fresh native-shape stub
                        // (so cross-realm Function.prototype.toString.call(this) returns
                        // "function NAME() { [native code] }").
                        newDesc.value = _mkNativeFn(propName);
                    } else {
                        newDesc.value = desc.value;
                    }
                }
                try { Object.defineProperty(freshProto, propName, newDesc); } catch (_) {}
            }
        }

        try {
            Object.defineProperty(freshProto, "constructor", {
                value: fresh, writable: true, enumerable: false, configurable: true,
            });
            Object.defineProperty(fresh, "prototype", {
                value: freshProto, writable: false, enumerable: false, configurable: false,
            });
        } catch (_) {}
        return fresh;
    }

    // For each mirrored constructor name, find the nearest ancestor in
    // _MIRRORED_CONSTRUCTORS by walking the real prototype chain. Returns
    // an array of names in topological order (ancestors before descendants)
    // and a name -> direct-parent-name map.
    function _topoSortMirrored(names) {
        const realCtors = {};
        for (const n of names) {
            try {
                const c = globalThis[n];
                if (typeof c === "function") realCtors[n] = c;
            } catch (_) {}
        }
        const directParent = {};
        for (const n of names) {
            const ctor = realCtors[n];
            if (!ctor) { directParent[n] = null; continue; }
            let proto = null;
            try { proto = Object.getPrototypeOf(ctor.prototype); } catch (_) {}
            let parentName = null;
            let guard = 0;
            while (proto && guard++ < 32) {
                for (const m of names) {
                    const mc = realCtors[m];
                    if (mc && mc.prototype === proto) { parentName = m; break; }
                }
                if (parentName) break;
                try { proto = Object.getPrototypeOf(proto); } catch (_) { break; }
            }
            directParent[n] = parentName;
        }
        const ordered = [];
        const remaining = new Set(names);
        while (remaining.size > 0) {
            let progress = false;
            for (const n of Array.from(remaining)) {
                const p = directParent[n];
                if (p == null || !remaining.has(p)) {
                    ordered.push(n);
                    remaining.delete(n);
                    progress = true;
                }
            }
            if (!progress) {
                // Defensive: cyclic dependency in the real prototype graph
                // shouldn't happen, but if it does, append remaining without
                // ordering rather than infinite-looping.
                for (const n of remaining) ordered.push(n);
                break;
            }
        }
        return { ordered: ordered, directParent: directParent };
    }

    // Module-level cache: every iframe in this realm shares the same set of
    // mirrored constructors. Some scripts tag function/descriptor objects on
    // a first scope-chain walk and re-read them on a later walk; without this
    // cache every _getIframeWindow() call rebuilt the realm and any such
    // sentinel property set by the script was lost on the second read.
    let _cachedRemoteRealm = null;

    function _buildRemoteRealm() {
        if (_cachedRemoteRealm) return _cachedRemoteRealm;
        const realm = {};
        const sorted = _topoSortMirrored(_MIRRORED_CONSTRUCTORS);
        for (const name of sorted.ordered) {
            try {
                const parentCtor = globalThis[name];
                if (typeof parentCtor !== "function") continue;
                const parentName = sorted.directParent[name];
                const freshGrandparentProto = parentName && realm[parentName]
                    ? realm[parentName].prototype
                    : Object.prototype;
                realm[name] = _mkMirroredConstructor(parentCtor, name, freshGrandparentProto);
            } catch (_) {}
        }
        _cachedRemoteRealm = realm;
        return realm;
    }

    // DOM/Web-interface constructors do not come with a bare V8 Context. Do
    // not alias the parent realm's constructor objects into the child: that
    // makes `iframe.contentWindow.Node === window.Node`, which is observable by
    // every serious fingerprint suite. Instead create realm-local forwarding
    // constructors and realm-local prototype objects inside the child context.
    // Their methods delegate to the already-tested parent implementations while
    // preserving child-realm function/prototype identity and native toString
    // shape.
    const _CHILD_REALM_INTERFACES = [
        "Navigator", "EventTarget", "Event", "CustomEvent", "MessageEvent",
        "Node", "Element", "HTMLElement", "Document", "HTMLDocument",
        "DocumentFragment", "Text", "Comment", "ShadowRoot", "SVGElement",
        "NodeList", "HTMLCollection", "DOMTokenList",
        "HTMLHtmlElement", "HTMLHeadElement", "HTMLBodyElement",
        "HTMLDivElement", "HTMLSpanElement", "HTMLParagraphElement", "HTMLHeadingElement",
        "HTMLAnchorElement", "HTMLImageElement",
        "HTMLInputElement", "HTMLFormElement", "HTMLButtonElement",
        "HTMLSelectElement", "HTMLTextAreaElement", "HTMLCanvasElement",
        "HTMLScriptElement", "HTMLStyleElement", "HTMLLinkElement", "HTMLMetaElement",
        "HTMLTableElement", "HTMLIFrameElement", "HTMLVideoElement", "HTMLAudioElement",
        "HTMLUListElement", "HTMLOListElement", "HTMLLIElement",
        "HTMLTableRowElement", "HTMLTableCellElement", "HTMLTableSectionElement",
        "HTMLLabelElement", "HTMLOptionElement", "HTMLTemplateElement",
        "HTMLPreElement", "HTMLQuoteElement",
    ];

    function _installChildRealmInterfaces(realmId) {
        const sources = Object.create(null);
        for (const name of _CHILD_REALM_INTERFACES) {
            try {
                const ctor = globalThis[name];
                if (typeof ctor === "function") sources[name] = ctor;
            } catch (_) {}
        }
        try {
            ops.op_set_child_realm_prop(realmId, "__oxideInterfaceSources", sources);
        } catch (_) {
            return;
        }

        const namesJson = JSON.stringify(_CHILD_REALM_INTERFACES);
        const illegalJson = JSON.stringify(Array.from(_ILLEGAL_CONSTRUCTORS));
        const code = `(function(){
            const sources=globalThis.__oxideInterfaceSources||{};
            const frameRegistryHook=globalThis.__oxideFrameRegistryHook;
            try{delete globalThis.__oxideFrameRegistryHook;}catch(_){}
            const names=${namesJson};
            const illegal=new Set(${illegalJson});
            const nativeTag=Symbol.for('__browser_oxide_native__');
            const built=Object.create(null);
            function nativeShape(fn,name){
                try{Object.defineProperty(fn,'name',{value:name,configurable:true});}catch(_){}
                try{Object.defineProperty(fn,nativeTag,{value:name,configurable:true});}catch(_){}
                return fn;
            }
            function syncFrames(){
                try{if(typeof frameRegistryHook==='function')frameRegistryHook();}catch(_){}
            }
            function callWrapper(fn,name){
                return nativeShape(function(...args){return Reflect.apply(fn,this,args);},name);
            }
            function copyDescriptor(desc,key){
                const out={configurable:desc.configurable!==false,enumerable:!!desc.enumerable};
                const label=typeof key==='symbol'?(key.description||'') : String(key);
                if('value' in desc){
                    out.writable=desc.writable!==false;
                    out.value=typeof desc.value==='function'?callWrapper(desc.value,label):desc.value;
                }else{
                    if(desc.get)out.get=callWrapper(desc.get,'get '+label);
                    if(desc.set)out.set=callWrapper(desc.set,'set '+label);
                }
                return out;
            }
            function directParentName(name){
                let proto=null;
                try{proto=Object.getPrototypeOf(sources[name].prototype);}catch(_){}
                let guard=0;
                while(proto&&guard++<32){
                    for(const candidate of names){
                        try{if(sources[candidate]&&sources[candidate].prototype===proto)return candidate;}catch(_){}
                    }
                    try{proto=Object.getPrototypeOf(proto);}catch(_){break;}
                }
                return null;
            }
            const pending=new Set(names.filter(name=>typeof sources[name]==='function'));
            while(pending.size){
                let progress=false;
                for(const name of Array.from(pending)){
                    const parentName=directParentName(name);
                    if(parentName&&pending.has(parentName))continue;
                    const source=sources[name];
                    let fresh;
                    if(illegal.has(name)){
                        fresh=nativeShape(function(){throw new TypeError("Failed to construct '"+name+"': Illegal constructor");},name);
                    }else{
                        fresh=nativeShape(function(...args){return Reflect.construct(source,args,new.target||fresh);},name);
                    }
                    const parentProto=parentName&&built[parentName]
                        ? built[parentName].prototype : Object.prototype;
                    const proto=Object.create(parentProto);
                    try{
                        for(const key of Reflect.ownKeys(source.prototype||{})){
                            if(key==='constructor')continue;
                            const desc=Object.getOwnPropertyDescriptor(source.prototype,key);
                            if(desc)Object.defineProperty(proto,key,copyDescriptor(desc,key));
                        }
                    }catch(_){}
                    try{Object.defineProperty(proto,'constructor',{value:fresh,writable:true,configurable:true});}catch(_){}
                    try{fresh.prototype=proto;}catch(_){}
                    try{
                        for(const key of Reflect.ownKeys(source)){
                            if(key==='prototype'||key==='name'||key==='length'||key==='caller'||key==='arguments')continue;
                            const desc=Object.getOwnPropertyDescriptor(source,key);
                            if(desc)Object.defineProperty(fresh,key,copyDescriptor(desc,key));
                        }
                    }catch(_){}
                    built[name]=fresh;
                    Object.defineProperty(globalThis,name,{value:fresh,writable:true,configurable:true});
                    pending.delete(name);progress=true;
                }
                if(!progress)break;
            }
            const connectedNodes=new WeakSet();
            function markTree(value,connected){
                if(!value||typeof value!=='object')return;
                try{connected?connectedNodes.add(value):connectedNodes.delete(value);}catch(_){}
                try{
                    const descendants=value.querySelectorAll&&value.querySelectorAll('*');
                    if(descendants)for(let i=0;i<descendants.length;i++){
                        connected?connectedNodes.add(descendants[i]):connectedNodes.delete(descendants[i]);
                    }
                }catch(_){}
            }
            function retarget(value,ctorName,connected){
                if(!value||typeof value!=='object')return value;
                let ctor=built[ctorName];
                if(!ctor&&value.nodeType===1)ctor=built.HTMLElement||built.Element;
                if(!ctor&&value.nodeType)ctor=built.Node;
                try{if(ctor&&ctor.prototype)Object.setPrototypeOf(value,ctor.prototype);}catch(_){}
                if(connected!==undefined)markTree(value,!!connected);
                return value;
            }
            if(built.EventTarget&&built.Event){
                const eventListeners=new WeakMap();
                function listenerList(target,type,create){
                    let byType=eventListeners.get(target);
                    if(!byType&&create){byType=new Map();eventListeners.set(target,byType);}
                    if(!byType)return null;
                    let list=byType.get(String(type));
                    if(!list&&create){list=[];byType.set(String(type),list);}
                    return list;
                }
                function addEventListener(type,callback,options){
                    if(!(typeof callback==='function'||(callback&&typeof callback.handleEvent==='function')))return;
                    const capture=typeof options==='boolean'?options:!!(options&&options.capture);
                    const once=!!(options&&typeof options==='object'&&options.once);
                    const passive=!!(options&&typeof options==='object'&&options.passive);
                    const list=listenerList(this,type,true);
                    if(list.some(item=>item.callback===callback&&item.capture===capture))return;
                    list.push({callback,capture,once,passive});
                }
                function removeEventListener(type,callback,options){
                    const capture=typeof options==='boolean'?options:!!(options&&options.capture);
                    const list=listenerList(this,type,false);if(!list)return;
                    const index=list.findIndex(item=>item.callback===callback&&item.capture===capture);
                    if(index>=0)list.splice(index,1);
                }
                let _reportingListenerError=false;
                const _redeliveredEvents=new WeakSet();
                function _reportListenerError(target,event,error){
                    if(_reportingListenerError)return;
                    _reportingListenerError=true;
                    try{
                        const isErrEvent=event&&event.type==='error';
                        if(!isErrEvent&&typeof globalThis.ErrorEvent==='function'){
                            const msg=(error&&error.message)?String(error.message):String(error);
                            const loc=(error&&error.stack)?String(error.stack).split('\n')[1]||'':'';
                            const m=loc&&loc.match(/(\d+):(\d+)\)?\s*$/);
                            const ev=new ErrorEvent('error',{
                                message:'Uncaught '+msg,
                                filename:target===globalThis?'':String((globalThis.location&&globalThis.location.href)||''),
                                lineno:m?+m[1]:0,
                                colno:m?+m[2]:0,
                                error:error,
                            });
                            _markFrameMessageTrusted&&_markFrameMessageTrusted(ev);
                            globalThis.dispatchEvent(ev);
                        }
                        if(typeof globalThis.console!=='undefined'&&console.error)console.error('Uncaught (in event listener)',error);
                    }catch(_){}
                    // A frame message handler that crashes on first delivery
                    // (init race inside the page's own code) leaves the
                    // widget's state machine wedged: every later message
                    // crashes on the same half-initialized state. Requeue the
                    // failed message once, delayed, so the handler reruns
                    // after the pending init has completed (observed to
                    // unstick Turnstile's parent->frame handshake).
                    try{
                        if(event&&event.type==='message'&&typeof setTimeout==='function'&&!_redeliveredEvents.has(event)){
                            _redeliveredEvents.add(event);
                            setTimeout(function(){
                                try{
                                    const t=(target&&typeof target.dispatchEvent==='function')?target:globalThis;
                                    t.dispatchEvent(event);
                                }catch(_){}
                            },1200);
                        }
                    }catch(_){}
                    _reportingListenerError=false;
                }
                function fire(target,event,capture){
                    if(!capture&&!event._stoppedImmediate){
                        const handler=target&&target['on'+event.type];
                        if(typeof handler==='function')handler.call(target,event);
                    }
                    const list=listenerList(target,event.type,false);if(!list)return;
                    const remove=[];
                    for(let i=0;i<list.length;i++){
                        const item=list[i];if(item.capture!==capture)continue;
                        if(event._stoppedImmediate)break;
                        // Browser semantics: an exception in one listener is
                        // reported as an uncaught error on the window and must
                        // not abort the dispatch — later listeners still run
                        // and dispatchEvent returns normally. Letting it
                        // propagate starves every later listener and kills
                        // the frame message pump (observed: Turnstile's
                        // parent->frame handshake dies on the first soft
                        // error and the widget loops on crashed_retry).
                        try{
                            if(typeof item.callback==='function')item.callback.call(target,event);
                            else item.callback.handleEvent(event);
                        }catch(listenerError){
                            _reportListenerError(target,event,listenerError);
                        }
                        if(item.once)remove.push(i);
                    }
                    for(let i=remove.length-1;i>=0;i--)list.splice(remove[i],1);
                }
                function dispatchEvent(event){
                    if(!(event instanceof built.Event))throw new TypeError("Failed to execute 'dispatchEvent' on 'EventTarget': parameter 1 is not of type 'Event'.");
                    try{event.target=this;}catch(_){}
                    const path=[];let current=this;
                    try{while(current){path.push(current);current=current.parentNode||null;}}catch(_){}
                    // DOM standard: the propagation path of an event at the
                    // Document continues through the window (capture inbound,
                    // bubble outbound). Window-level lifecycle listeners
                    // (DOMContentLoaded et al) never see document events
                    // without this, because document.parentNode is null.
                    try{
                        const doc=globalThis.document;
                        if(path.length>0&&path[path.length-1]===doc&&globalThis.window&&typeof globalThis.window.addEventListener==='function'&&path.indexOf(globalThis.window)<0)path.push(globalThis.window);
                    }catch(_){}
                    if(path.length>1&&!event._stopped){
                        for(let i=path.length-1;i>0;i--){event.currentTarget=path[i];event.eventPhase=1;fire(path[i],event,true);if(event._stopped)break;}
                    }
                    if(!event._stopped){event.currentTarget=this;event.eventPhase=2;fire(this,event,false);fire(this,event,true);}
                    if(path.length>1&&!event._stopped&&event.bubbles){
                        for(let i=1;i<path.length;i++){event.currentTarget=path[i];event.eventPhase=3;fire(path[i],event,false);if(event._stopped)break;}
                    }
                    event.currentTarget=null;event.eventPhase=0;
                    return !event.defaultPrevented;
                }
                try{Object.defineProperty(built.EventTarget.prototype,'addEventListener',{value:nativeShape(addEventListener,'addEventListener'),writable:true,enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.EventTarget.prototype,'removeEventListener',{value:nativeShape(removeEventListener,'removeEventListener'),writable:true,enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.EventTarget.prototype,'dispatchEvent',{value:nativeShape(dispatchEvent,'dispatchEvent'),writable:true,enumerable:true,configurable:true});}catch(_){}
            }
            if(built.Node){
                try{Object.defineProperty(built.Node.prototype,'ownerDocument',{get:nativeShape(function(){return globalThis.document;},'get ownerDocument'),enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.Node.prototype,'isConnected',{get:nativeShape(function(){return connectedNodes.has(this);},'get isConnected'),enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.Node.prototype,'baseURI',{get:nativeShape(function(){return String(globalThis.location&&globalThis.location.href||'about:blank');},'get baseURI'),enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.Node.prototype,'getRootNode',{value:nativeShape(function(options){return connectedNodes.has(this)?globalThis.document:this;},'getRootNode'),writable:true,configurable:true});}catch(_){}
                try{
                    const append=built.Node.prototype.appendChild;
                    if(typeof append==='function')Object.defineProperty(built.Node.prototype,'appendChild',{value:nativeShape(function(child){const out=append.call(this,child);if(connectedNodes.has(this))markTree(child,true);syncFrames();return out;},'appendChild'),writable:true,configurable:true});
                    const insert=built.Node.prototype.insertBefore;
                    if(typeof insert==='function')Object.defineProperty(built.Node.prototype,'insertBefore',{value:nativeShape(function(child,before){const out=insert.call(this,child,before);if(connectedNodes.has(this))markTree(child,true);syncFrames();return out;},'insertBefore'),writable:true,configurable:true});
                    const remove=built.Node.prototype.removeChild;
                    if(typeof remove==='function')Object.defineProperty(built.Node.prototype,'removeChild',{value:nativeShape(function(child){const out=remove.call(this,child);markTree(child,false);syncFrames();return out;},'removeChild'),writable:true,configurable:true});
                }catch(_){}
            }
            if(built.Element){
                try{
                    const remove=built.Element.prototype.remove;
                    if(typeof remove==='function')Object.defineProperty(built.Element.prototype,'remove',{value:nativeShape(function(){const out=remove.call(this);markTree(this,false);syncFrames();return out;},'remove'),writable:true,configurable:true});
                    const before=built.Element.prototype.before;
                    if(typeof before==='function')Object.defineProperty(built.Element.prototype,'before',{value:nativeShape(function(...nodes){const parent=this.parentNode;const connected=!!parent&&connectedNodes.has(parent);const out=before.apply(this,nodes);if(connected)for(const node of nodes)if(node&&typeof node==='object')markTree(node,true);syncFrames();return out;},'before'),writable:true,configurable:true});
                    const after=built.Element.prototype.after;
                    if(typeof after==='function')Object.defineProperty(built.Element.prototype,'after',{value:nativeShape(function(...nodes){const parent=this.parentNode;const connected=!!parent&&connectedNodes.has(parent);const out=after.apply(this,nodes);if(connected)for(const node of nodes)if(node&&typeof node==='object')markTree(node,true);syncFrames();return out;},'after'),writable:true,configurable:true});
                    const replaceWith=built.Element.prototype.replaceWith;
                    if(typeof replaceWith==='function')Object.defineProperty(built.Element.prototype,'replaceWith',{value:nativeShape(function(...nodes){const parent=this.parentNode;const connected=!!parent&&connectedNodes.has(parent);const out=replaceWith.apply(this,nodes);markTree(this,false);if(connected)for(const node of nodes)if(node&&typeof node==='object')markTree(node,true);syncFrames();return out;},'replaceWith'),writable:true,configurable:true});
                    const append=built.Element.prototype.append;
                    if(typeof append==='function')Object.defineProperty(built.Element.prototype,'append',{value:nativeShape(function(...nodes){const out=append.apply(this,nodes);if(connectedNodes.has(this))for(const node of nodes)if(node&&typeof node==='object')markTree(node,true);syncFrames();return out;},'append'),writable:true,configurable:true});
                    const prepend=built.Element.prototype.prepend;
                    if(typeof prepend==='function')Object.defineProperty(built.Element.prototype,'prepend',{value:nativeShape(function(...nodes){const out=prepend.apply(this,nodes);if(connectedNodes.has(this))for(const node of nodes)if(node&&typeof node==='object')markTree(node,true);syncFrames();return out;},'prepend'),writable:true,configurable:true});
                    const insertAdjacentHTML=built.Element.prototype.insertAdjacentHTML;
                    if(typeof insertAdjacentHTML==='function')Object.defineProperty(built.Element.prototype,'insertAdjacentHTML',{value:nativeShape(function(position,html){const out=insertAdjacentHTML.call(this,position,html);syncFrames();return out;},'insertAdjacentHTML'),writable:true,configurable:true});
                    const innerHTML=Object.getOwnPropertyDescriptor(built.Element.prototype,'innerHTML');
                    if(innerHTML&&typeof innerHTML.set==='function')Object.defineProperty(built.Element.prototype,'innerHTML',{
                        get:innerHTML.get,
                        set:nativeShape(function(value){const out=innerHTML.set.call(this,value);syncFrames();return out;},'set innerHTML'),
                        enumerable:!!innerHTML.enumerable,
                        configurable:true,
                    });
                }catch(_){}
            }
            if(built.Document){
                try{Object.defineProperty(built.Document.prototype,'ownerDocument',{get:nativeShape(function(){return null;},'get ownerDocument'),enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.Document.prototype,'isConnected',{get:nativeShape(function(){return true;},'get isConnected'),enumerable:true,configurable:true});}catch(_){}
                try{Object.defineProperty(built.Document.prototype,'getRootNode',{value:nativeShape(function(){return this;},'getRootNode'),writable:true,configurable:true});}catch(_){}
            }
            Object.defineProperty(globalThis,'__oxideDomRetargetValue',{value:nativeShape(retarget,'retargetDomValue'),configurable:true});
            try{if(globalThis.navigator&&built.Navigator)Object.setPrototypeOf(globalThis.navigator,built.Navigator.prototype);}catch(_){}
            try{if(globalThis.document&&built.Document)Object.setPrototypeOf(globalThis.document,built.Document.prototype);}catch(_){}
            try{if(document.documentElement)retarget(document.documentElement,'HTMLHtmlElement',true);}catch(_){}
            try{if(document.head)retarget(document.head,'HTMLHeadElement',true);}catch(_){}
            try{if(document.body)retarget(document.body,'HTMLBodyElement',true);}catch(_){}
        })();`;
        try { ops.op_eval_in_child_realm(realmId, code); } catch (_) {}
        try { ops.op_delete_child_realm_prop(realmId, "__oxideInterfaceSources"); } catch (_) {}
    }

    // Monotonically-increasing ID for child realms; used as the Rust-side
    // cache key in IframeRealmStore (HashMap<u32, ...>).
    let _nextRealmId = 0;

    // Cap nested iframe materialization depth and skip a URL already in the
    // current chain so a frame cycle can't recurse into a stack overflow.
    let _ifMatDepth = 0;
    const _IF_MAT_MAX = 6;
    const _ifMatInProgress = new Set();

    // Realm id whose frame scripts are currently running (`null` = top page), so
    // an iframe they insert is tagged with its true parent realm for postMessage routing.
    let _currentMatRealmId = null;
    // realm id -> the frame's document URL, so a script that frame inserts
    // resolves its relative src against the frame's own origin.
    const _realmBaseUrl = new Map();

    // Frame registry: window[0], window[1], ... and window.length.
    // Some scripts access child iframes via window[N]
    // (frames[N]), NOT via iframe.contentWindow. Real Chrome updates
    // window[N] and window.length when iframes are appended to the DOM.
    const _frameRegistry = [];

    function _syncFrameRegistryForRealm(realmId) {
        if (_realmRegistrySyncing.has(realmId)) return;
        const doc = _realmDocumentById.get(realmId);
        if (!doc || typeof doc.querySelectorAll !== "function") return;

        _realmRegistrySyncing.add(realmId);
        try {

            let frames = [];
            try { frames = doc.querySelectorAll("iframe") || []; } catch (_) { frames = []; }
            const previousHosts = _realmFrameHosts.get(realmId) || new Map();
            const previousNamed = _realmFrameNamedSlots.get(realmId) || new Set();
            const currentHosts = new Map();
            const nextNamed = new Map();
            let slot = 0;

            for (let i = 0; i < frames.length; i++) {
                const frame = frames[i];
                const nodeId = _iframeNodeId(frame);
                if (nodeId === null) continue;
                // Parser-created/innerHTML/document.write iframes never pass
                // through Document.createElement(), so stamp their owning realm
                // here before contentWindow materialization/registration.
                try { frame.__oxParentRealm = realmId; } catch (_) {}
                currentHosts.set(nodeId, frame);
                try {
                    const name = String((frame.getAttribute && frame.getAttribute("name")) || frame.name || "");
                    if (name && !/^(0|[1-9]\d*)$/.test(name) && !nextNamed.has(name)) {
                        nextNamed.set(name, frame);
                    }
                } catch (_) {}
                let publicWindow = null;
                try {
                    const backend = _getIframeWindow(frame);
                    publicWindow = _publicWindowProxyFor(frame, backend);
                } catch (_) {}
                if (!publicWindow) continue;
                try { ops.op_set_child_realm_prop(realmId, String(slot), publicWindow); } catch (_) {}
                slot++;
            }

            // Removed nested frames lose their browsing contexts and numeric slots.
            for (const [nodeId, oldFrame] of previousHosts) {
                if (currentHosts.has(nodeId)) continue;
                try { _disposeIframeRealm(oldFrame, false); } catch (_) {}
            }
            const previousCount = _realmFrameSlotCount.get(realmId) || 0;
            for (let i = slot; i < previousCount; i++) {
                try { ops.op_delete_child_realm_prop(realmId, String(i)); } catch (_) {}
            }
            try { ops.op_set_child_realm_prop(realmId, "length", slot); } catch (_) {}

            for (const name of previousNamed) {
                if (nextNamed.has(name)) continue;
                try { ops.op_delete_child_realm_prop(realmId, name); } catch (_) {}
            }
            const ownedNames = new Set(previousNamed);
            const installedNames = new Set();
            for (const [name, frame] of nextNamed) {
                let publicWindow = null;
                try {
                    const backend = _getIframeWindow(frame);
                    publicWindow = _publicWindowProxyFor(frame, backend);
                } catch (_) {}
                if (!publicWindow) continue;
                try {
                    const installed = ownedNames.has(name)
                        ? (ops.op_set_child_realm_prop(realmId, name, publicWindow), true)
                        : ops.op_set_child_realm_prop_if_absent(realmId, name, publicWindow);
                    if (installed) installedNames.add(name);
                } catch (_) {}
            }
            _realmFrameHosts.set(realmId, currentHosts);
            _realmFrameSlotCount.set(realmId, slot);
            _realmFrameNamedSlots.set(realmId, installedNames);
        } finally {
            _realmRegistrySyncing.delete(realmId);
        }
    }

    function _syncOwningFrameRegistry(el) {
        const parentRealmId = (el && typeof el.__oxParentRealm === "number")
            ? el.__oxParentRealm : null;
        if (parentRealmId !== null) _syncFrameRegistryForRealm(parentRealmId);
        else _syncTopFrameRegistry();
    }

    // Register contentWindow cw at frame index _fi in the main window.
    // Pass the iframe element el so we can find its DOM position and also
    // handle cases where the iframe was inserted via a non-tracked method
    // (insertBefore, innerHTML, insertAdjacentHTML, etc.).
    function _registerFrame(cw, el) {
        const parentRealmId = (el && typeof el.__oxParentRealm === "number")
            ? el.__oxParentRealm : null;
        if (parentRealmId !== null) {
            _syncFrameRegistryForRealm(parentRealmId);
            return;
        }
        // Indexed Window access is defined over *document-tree* child
        // navigables. An iframe in a shadow tree has a real content navigable,
        // but it is not a document-tree descendant and therefore must not
        // consume window[n]/window.length slots.
        try {
            if (el && typeof el.getRootNode === "function" && el.getRootNode() !== document) {
                return;
            }
        } catch (_) {}
        // Try to find the iframe's true DOM position
        var _fi = -1;
        // First: check if el is already tracked in _appendedIframes
        if (el) {
            for (var _ai = 0; _ai < _appendedIframes.length; _ai++) {
                if (_appendedIframes[_ai] === el) { _fi = _ai; break; }
            }
        }
        // Second: if not tracked, query the DOM for its position
        if (_fi < 0) {
            try {
                var _all = document.getElementsByTagName && document.getElementsByTagName('iframe');
                if (_all) {
                    for (var _di = 0; _di < _all.length; _di++) {
                        if (_all[_di] === el) { _fi = _di; break; }
                    }
                }
            } catch (_) {}
        }
        // Fallback: use sequential registry length
        if (_fi < 0) {
            _fi = _frameRegistry.length;
        }
        // Track in registry
        while (_frameRegistry.length <= _fi) _frameRegistry.push(null);
        _frameRegistry[_fi] = cw;
        // Register in _appendedIframes if not already there (for lazy getter)
        if (el && _fi >= _appendedIframes.length) {
            while (_appendedIframes.length < _fi) _appendedIframes.push(null);
            _appendedIframes.push(el);
            try { globalThis.__ifAppendCount = _appendedIframes.length; } catch (_) {}
        }
        // Install as window[N] — replace lazy getter (if any) with actual value
        try {
            Object.defineProperty(globalThis, String(_fi), {
                value: cw, writable: true, enumerable: true, configurable: true,
            });
        } catch (_) {}
        // Update window.length
        var _newLen = _fi + 1;
        try {
            const _ld = Object.getOwnPropertyDescriptor(globalThis, 'length');
            if (_ld && _ld.writable) {
                if (globalThis.length < _newLen) globalThis.length = _newLen;
            } else {
                Object.defineProperty(globalThis, 'length', {
                    value: _newLen, writable: true, configurable: true, enumerable: true,
                });
            }
        } catch (_) {}
    }

    // Extract scheme+host+port from a URL without using new URL().
    // Returns "null" for non-http(s) URLs (data:, about:, etc.) or empty input.
    const _xOrigin = function(u) {
        var m = u && u.match(/^(https?:\/\/[^/?#:]+(?::\d+)?)/i);
        return m ? m[1].toLowerCase() : "null";
    };

    function _getIframeWindow(el) {
        // Snapshot the embedding realm's location before creating/entering the
        // child V8 context. Values read after `op_create_child_realm` can be
        // resolved against the new about:blank global on some V8 paths.
        const _embeddingLocation = (() => {
            try {
                const href = String(globalThis.location && globalThis.location.href || "about:blank");
                const parsed = new URL(href);
                return {
                    href,
                    origin: parsed.origin,
                    host: parsed.host,
                    hostname: parsed.hostname,
                    port: parsed.port,
                    protocol: parsed.protocol,
                };
            } catch (_) {
                return { href: "about:blank", origin: "null", host: "", hostname: "", port: "", protocol: "about:" };
            }
        })();
        let state = _getIframeState(el);
        if (state) {
            // Rebuild the realm only when the src actually changed, not on every
            // access, so a cross-origin frame's running app isn't wiped.
            try {
                const _cSrc = (el && typeof el.getAttribute === "function")
                    ? (el.getAttribute("src") || el.src || "")
                    : (el && el.src || "");
                if (state._src !== undefined && _cSrc !== state._src) {
                    _disposeIframeRealm(el, true);
                    state = undefined;
                }
            } catch (_) {}
            // Re-run srcdoc scripts if srcdoc was set after initial contentWindow access.
            // A script may set iframe.srcdoc = "..." before or after first contentWindow
            // access; in either case we must execute the scripts in the child realm.
            if (state && state._realmId !== undefined) {
                let _cur = "";
                try { _cur = el.getAttribute("srcdoc") || el.srcdoc || ""; } catch (_) {}
                if (_cur && _cur !== state._processedSrcdoc) {
                    state._processedSrcdoc = _cur;
                    try {
                        const _re = /<script[^>]*>([\s\S]*?)<\/script>/gi;
                        let _m2;
                        while ((_m2 = _re.exec(_cur)) !== null) {
                            const _s2 = _m2[1];
                            if (_s2 && _s2.trim()) {
                                try { ops.op_eval_in_child_realm(state._realmId, _s2); } catch (_) {}
                            }
                        }
                    } catch (_) {}
                }
            }
            if (state) return state.contentWindow;
        }

        // ── Build the iframe document shell ──────────────────────────────
        // srcdoc iframes: expose the source text for
        // reads (`iframe.contentDocument.body.innerHTML`).
        let _srcdoc = "";
        try {
            // IDL property is the authoritative reflected value. Fall back to
            // the raw attribute for parser-created iframes before the setter has
            // ever run.
            if (el && typeof el.srcdoc === "string") {
                _srcdoc = el.srcdoc;
            }
            if (!_srcdoc && el && typeof el.getAttribute === "function") {
                _srcdoc = el.getAttribute("srcdoc") || "";
            }
        } catch (_) {}
        const _mkHtmlMirror = (tag, inner) => ({
            tagName: tag.toUpperCase(),
            nodeType: 1,
            innerHTML: inner,
            outerHTML: "<" + tag + ">" + inner + "</" + tag + ">",
            textContent: "",
            children: [],
            childNodes: [],
            firstChild: null, lastChild: null,
            parentNode: null,
            getAttribute() { return null; },
            setAttribute() {},
            hasAttribute() { return false; },
            appendChild(_c) {},
            removeChild(_c) {},
        });
        // Parse srcdoc into a detached, real DOM subtree. Sharing the arena is
        // safe because disconnected nodes are outside the top document's query
        // scope, while the child document still gets full selector/mutation
        // semantics. This replaces the old string-only mirror that could not
        // support getElementById/querySelector or script-driven DOM changes.
        let _docEl = null, _body = null, _head = null, _queryRoot = null, _realDoc = false;
        if (_srcdoc) {
            try {
                const root = _document.createElement("div");
                root.innerHTML = _srcdoc;
                _queryRoot = root;
                _docEl = root.querySelector("html") || root;
                _head = root.querySelector("head") || _mkHtmlMirror("head", "");
                _body = root.querySelector("body") || root;
                _realDoc = true;
            } catch (_) {
                _docEl = _mkHtmlMirror("html", _srcdoc);
                _body = _mkHtmlMirror("body", _srcdoc);
                _head = _mkHtmlMirror("head", "");
            }
        } else {
            try {
                _docEl = _document.createElement("html");
                _head = _document.createElement("head");
                _body = _document.createElement("body");
                _docEl.appendChild(_head);
                _docEl.appendChild(_body);
                _queryRoot = _docEl;
                _realDoc = true;
            } catch (_) { _docEl = null; _body = null; _head = null; }
        }
        let _retargetDomValue = function(value) { return value; };
        const _childCtorName = (value) => {
            if (!value || typeof value !== "object") return "Object";
            // Fresh nodes produced by innerHTML/HTML parsing may still carry a
            // generic Element/HTMLElement prototype when first observed from
            // the child realm. The Rust DOM tag is the authoritative type;
            // derive the realm-local constructor from it before trusting the
            // wrapper's current prototype chain.
            try {
                if (value.nodeType === 1) {
                    const tag = ops.op_dom_get_tag_name(_getNodeId(value)).toLowerCase();
                    const proto = _tagToProto[tag] || HTMLElement.prototype;
                    const name = proto && proto.constructor && proto.constructor.name;
                    if (name) return String(name);
                }
            } catch (_) {}
            try {
                const name = value.constructor && value.constructor.name;
                if (name) return String(name);
            } catch (_) {}
            if (value.nodeType === 3) return "Text";
            if (value.nodeType === 8) return "Comment";
            if (value.nodeType === 11) return "DocumentFragment";
            if (value.nodeType === 1) return "HTMLElement";
            return "Object";
        };
        const _retargetChildNode = (value, connected = true) => {
            if (!value) return value;
            try { return _retargetDomValue(value, _childCtorName(value), connected); }
            catch (_) { return value; }
        };
        const _retargetChildCollection = (collection, connected = true) => {
            if (!collection) return collection;
            try {
                for (let i = 0; i < collection.length; i++) {
                    _retargetChildNode(collection[i], connected);
                }
                return _retargetDomValue(collection, _childCtorName(collection), false);
            } catch (_) { return collection; }
        };
        let _childDocumentWriteDepth = 0;
        const _CHILD_DOCUMENT_WRITE_MAX = 32;
        const iframeDoc = {
            documentElement: _docEl,
            head: _head,
            body: _body,
            title: "",
            readyState: "loading",
            visibilityState: "visible",
            hidden: false,
            hasFocus() { return false; },
            querySelector(s) { return _realDoc && _queryRoot ? _retargetChildNode(_queryRoot.querySelector(s), true) : null; },
            querySelectorAll(s) { return _realDoc && _queryRoot ? _retargetChildCollection(_queryRoot.querySelectorAll(s), true) : new NodeList([]); },
            getElementById(id) { return _realDoc && _queryRoot ? _retargetChildNode(_queryRoot.querySelector('[id="' + String(id).replace(/"/g, '\\"') + '"]'), true) : null; },
            getElementsByTagName(tag) {
                const t = String(tag).toLowerCase();
                if (_srcdoc && t === "html" && _docEl) return _retargetChildCollection(new NodeList([_getNodeId(_docEl)], 2), true);
                if (_srcdoc && t === "body" && _body) return _retargetChildCollection(new NodeList([_getNodeId(_body)], 2), true);
                if (_srcdoc && t === "head" && _head) return _retargetChildCollection(new NodeList([_getNodeId(_head)], 2), true);
                if (_realDoc && _queryRoot) {
                    if (t === "html" && _docEl) return _retargetChildCollection(new NodeList([_getNodeId(_docEl)], 2), true);
                    if (t === "body") return _retargetChildCollection(new NodeList([_getNodeId(_body)], 2), true);
                    if (t === "head" && _head) return _retargetChildCollection(new NodeList([_getNodeId(_head)], 2), true);
                    return _retargetChildCollection(_queryRoot.getElementsByTagName(tag), true);
                }
                return new NodeList([]);
            },
            createElement(tag) {
                const node = _retargetChildNode(_document.createElement(tag), false);
                try {
                    if (String(tag).toLowerCase() === 'iframe') node.__oxParentRealm = _realmId;
                } catch (_) {}
                return node;
            },
            createElementNS(ns, tag) {
                const node = _retargetChildNode(_document.createElementNS(ns, tag), false);
                try {
                    if (String(tag).toLowerCase() === 'iframe') node.__oxParentRealm = _realmId;
                } catch (_) {}
                return node;
            },
            createEvent(type) { return _document.createEvent(type); },
            createRange() { return _document.createRange(); },
            createTextNode(text) { return _retargetChildNode(_document.createTextNode(text), false); },
            write(html) {
                if (!_realDoc || !_body || _childDocumentWriteDepth >= _CHILD_DOCUMENT_WRITE_MAX) return;
                _childDocumentWriteDepth++;
                try {
                    const markup = String(html);
                    const existingScripts = new Set();
                    try {
                        const before = _body.querySelectorAll ? _body.querySelectorAll("script") : [];
                        for (let i = 0; i < before.length; i++) existingScripts.add(_getNodeId(before[i]));
                    } catch (_) {}

                    try { _body.insertAdjacentHTML("beforeend", markup); }
                    catch (_) { _body.innerHTML = String(_body.innerHTML || "") + markup; }

                    try {
                        const descendants = _body.querySelectorAll ? _body.querySelectorAll("*") : [];
                        _retargetChildCollection(descendants, true);
                    } catch (_) {}
                    try { _syncFrameRegistryForRealm(_realmId); } catch (_) {}

                    // document.write parser-inserted scripts execute
                    // synchronously. Only execute nodes created by this write;
                    // previously written scripts must not run twice.
                    try {
                        const scripts = _body.querySelectorAll ? _body.querySelectorAll("script") : [];
                        const parentRealmId = (el && typeof el.__oxParentRealm === "number")
                            ? el.__oxParentRealm : null;
                        const baseUrl = _realmBaseUrl.get(_realmId)
                            || (parentRealmId !== null ? _realmBaseUrl.get(parentRealmId) : "")
                            || _embeddingLocation.href;
                        for (let i = 0; i < scripts.length; i++) {
                            const script = scripts[i];
                            const nodeId = _getNodeId(script);
                            if (existingScripts.has(nodeId)) continue;
                            const type = String((script.getAttribute && script.getAttribute("type")) || "").toLowerCase();
                            if (type && type !== "text/javascript" && type !== "application/javascript" && type !== "module") continue;
                            let code = "";
                            const src = (script.getAttribute && script.getAttribute("src")) || "";
                            if (src) {
                                let absolute = src;
                                try { absolute = new URL(src, baseUrl).href; } catch (_) {}
                                try { code = ops.op_net_fetch_frame_sync(absolute, baseUrl) || ""; } catch (_) {}
                            } else {
                                try { code = script.textContent || ""; } catch (_) {}
                            }
                            if (code && String(code).trim()) {
                                try { ops.op_eval_in_child_realm(_realmId, String(code)); } catch (_) {}
                            }
                        }
                    } catch (_) {}
                } finally {
                    _childDocumentWriteDepth--;
                }
            },
            writeln(html) { return iframeDoc.write(String(html) + "\n"); },
            open() { return _document.open(); },
            close() { return _document.close(); },
        };

        // ── Screen mirror ─────────────────────────────────────────────────
        const _parentScreen = globalThis.screen || {};
        const _iframeScreen = {
            availWidth:  _parentScreen.availWidth  || 1920,
            availHeight: _parentScreen.availHeight || 1080,
            width:       _parentScreen.width       || 1920,
            height:      _parentScreen.height      || 1080,
            availLeft:   _parentScreen.availLeft   || 0,
            availTop:    _parentScreen.availTop    || 0,
            colorDepth:  _parentScreen.colorDepth  || 24,
            pixelDepth:  _parentScreen.pixelDepth  || 24,
            orientation: _parentScreen.orientation,
        };
        if (!/Firefox\/|Gecko\/20100101/.test(
            (typeof navigator !== "undefined" && navigator.userAgent) || ""
        )) {
            _iframeScreen.isExtended = false;
        }

        // ── Obtain the child window object ───────────────────────────────
        // PRIMARY PATH: genuine v8::Context child realm.
        // op_create_child_realm returns the child global:
        //   - Real, realm-distinct native intrinsics (Object/Function/… ≠ parent)
        //   - constructor.name === "Window" (set up in Rust)
        //   - Genuine-native Function.prototype.toString in child realm
        //   - self/window/globalThis/frames self-refs (set in Rust)
        // Matches real Chrome, where contentWindow is a genuine realm rather
        // than a Proxy or a parent alias.
        const _realmId = _nextRealmId++;
        _realmBaseUrl.set(_realmId, _embeddingLocation.href);
        try {
            const nestedFrames = _queryRoot && _queryRoot.querySelectorAll
                ? _queryRoot.querySelectorAll('iframe') : [];
            for (let i = 0; i < nestedFrames.length; i++) {
                nestedFrames[i].__oxParentRealm = _realmId;
            }
        } catch (_) {}
        let cw = null;
        let contentBackend = null;
        try {
            const _got = ops.op_create_child_realm(_realmId, _getNodeId(el));
            if (_got && typeof _got === "object") cw = _got;
            if (cw) {
                const inner = ops.op_child_realm_inner_global(_realmId);
                if (inner && typeof inner === "object") contentBackend = inner;
            }
        } catch (_) {}

        const _parentRealmId = (el && typeof el.__oxParentRealm === "number")
            ? el.__oxParentRealm : null;

        if (cw) {
            _realmDocumentById.set(_realmId, iframeDoc);
            // Publish an in-flight browsing-context record immediately after
            // the V8 child context exists. Setup below can re-enter frame
            // registry/contentWindow code; without this provisional state the
            // same host recursively creates realm 2, 3, ... before realm 1 is
            // committed. NodeId-backed state makes all re-entrant reads reuse
            // the one canonical child context.
            state = {
                contentWindow: cw,
                contentBackend: contentBackend || cw,
                contentDocument: iframeDoc,
                _realmId,
                _parentRealmId,
                _processedSrcdoc: _srcdoc,
                _src: ((el && el.getAttribute && el.getAttribute("src")) || (el && el.src) || ""),
                _initializing: true,
            };
            _setIframeState(el, state);
            // Properties that must be visible to code running INSIDE the child
            // realm are always installed through the Rust bridge. Declare the
            // helper before any optional debug/property population so nested
            // iframe materialization cannot hit a temporal-dead-zone error.
            const _sp = (k, v) => {
                try { ops.op_set_child_realm_prop(_realmId, k, v); } catch (_) {}
            };
            // Eval-source tap: challenge VMs (Turnstile) assemble their whole
            // program with `new Function` INSIDE this realm and crash inside
            // the result ('call' at <anonymous>:N). The parent-realm tap never
            // sees those realms, so install the same recorder here, before any
            // frame script can run. `String(fn)` keeps a single source of truth
            // with the main-realm definition at the bottom of this file.
            try {
                ops.op_eval_in_child_realm(
                    _realmId,
                    "(" + String(globalThis.__oxInstallEvalTap) + ")()",
                );
            } catch (_) {}
            // ── Populate child realm with DOM/FP properties ───────────────
            // CRITICAL: use op_set_child_realm_prop for properties that must be
            // visible to code running INSIDE the child realm (e.g. srcdoc
            // script eval). Direct `cw.x = v` from parent JS goes to the global PROXY's
            // own dict; code inside the realm reads from the INNER global.
            // op_set_child_realm_prop enters the child ContextScope and calls
            // child_global.set() which forwards via [[Set]] to the inner global.
            // iframeDoc back-reference to default view (set before _sp calls)
            try { iframeDoc.defaultView = cw; } catch (_) {}

            // Document
            _sp("document", iframeDoc);
            // Same-origin child windows expose their navigable container via
            // `window.frameElement`. Keep the element in its parent realm; a
            // nested child therefore observes the exact iframe object owned by
            // its immediate parent document.
            _sp("frameElement", el);

            // Location stub — about:blank inherits the parent origin per HTML spec.
            // Some scripts read document.domain (= hostname) and
            // location.origin; empty values differ from real Chrome.
            _sp("location", {
                href: "about:blank",
                origin: _embeddingLocation.origin,
                pathname: "/",
                hash: "", search: "",
                host: _embeddingLocation.host,
                hostname: _embeddingLocation.hostname,
                port: _embeddingLocation.port,
                protocol: _embeddingLocation.protocol,
                assign() {}, replace() {}, reload() {},
                toString() { return "about:blank"; },
            });

            // window.name reflects the iframe's name attribute; frame controllers
            // read it as a channel id, so an empty name breaks the handshake.
            let _frameName = "";
            try { _frameName = (el && el.getAttribute && el.getAttribute("name")) || (el && el.name) || ""; } catch (_) {}
            // parent / top are installed as context-aware accessors below
            // (op_install_frame_parent), after the postMessage proxies exist.
            _sp("name", _frameName);

            // Screen mirror (some scripts read these from inside child realm)
            _sp("screen", _iframeScreen);
            _sp("availWidth",  _iframeScreen.availWidth);
            _sp("availHeight", _iframeScreen.availHeight);

            // Viewport dimensions
            _sp("innerWidth",   globalThis.innerWidth  || 1920);
            _sp("innerHeight",  globalThis.innerHeight || 1080);
            _sp("outerWidth",   globalThis.outerWidth  || 1920);
            _sp("outerHeight",  globalThis.outerHeight || 1080);
            _sp("scrollX", 0); _sp("scrollY", 0);
            _sp("pageXOffset", 0); _sp("pageYOffset", 0);
            // Window state properties some scripts expect to be present.
            _sp("closed", false);
            _sp("name", _frameName);
            _sp("status", "");
            _sp("defaultStatus", "");
            _sp("screenTop", globalThis.screenTop || 0);
            _sp("screenLeft", globalThis.screenLeft || 0);
            _sp("screenX", globalThis.screenX || 0);
            _sp("screenY", globalThis.screenY || 0);
            // history stub — basic object so `.toString()` doesn't throw.
            _sp("history", { length: 0, state: null, scrollRestoration: "auto",
                back() {}, forward() {}, go() {}, pushState() {}, replaceState() {} });
            // Storage stubs — some scripts may call `.toString()` on these.
            const _storageStub = Object.create(null);
            Object.defineProperty(_storageStub, Symbol.toStringTag, { value: "Storage", configurable: true });
            _storageStub.length = 0;
            _storageStub.getItem = function getItem() { return null; };
            _storageStub.setItem = function setItem() {};
            _storageStub.removeItem = function removeItem() {};
            _storageStub.clear = function clear() {};
            _storageStub.key = function key() { return null; };
            try { _sp("localStorage", _storageStub); } catch (_) {}
            try { _sp("sessionStorage", _storageStub); } catch (_) {}
            // indexedDB — basic stub so typeof is "object".
            _sp("indexedDB", { open() {}, deleteDatabase() {}, databases() { return Promise.resolve([]); }, cmp() { return 0; } });
            // visualViewport — propagate from parent (some scripts may call .toString()).
            try { if (globalThis.visualViewport !== undefined) _sp("visualViewport", globalThis.visualViewport); } catch (_) {}

            // Event handler stubs — Chrome defines all on* handlers as null (data property,
            // enumerable:true) on the Window global. The child realm gets genuine V8 natives
            // but NOT these Window interface additions. Some scripts iterate the parent
            // window's enumerable properties and for each key check it in the child realm;
            // calling .toString() on the undefined value throws, while null.toString()
            // would throw too but with the correct Chrome-matching TypeError shape.
            // Setting them null here makes child[key] !== undefined for all on* keys.
            const _onHandlers = [
                'onabort','onafterprint','onanimationcancel','onanimationend',
                'onanimationiteration','onanimationstart','onappinstalled','onauxclick',
                'onbeforeinput','onbeforeinstallprompt','onbeforematch','onbeforeprint',
                'onbeforetoggle','onbeforeunload','onbeforexrselect','onblur',
                'oncancel','oncanplay','oncanplaythrough','onchange',
                'onclick','onclose','oncommand','oncontentvisibilityautostatechange',
                'oncontextlost','oncontextmenu','oncontextrestored','oncuechange',
                'ondblclick','ondrag','ondragend','ondragenter',
                'ondragleave','ondragover','ondragstart','ondrop',
                'ondurationchange','onemptied','onended','onfocus',
                'onformdata','ongamepadconnected','ongamepaddisconnected','ongotpointercapture',
                'onhashchange','oninput','oninvalid','onkeydown',
                'onkeypress','onkeyup','onlanguagechange','onload',
                'onloadeddata','onloadedmetadata','onloadstart','onlostpointercapture',
                'onmessage','onmessageerror','onmousedown','onmouseenter',
                'onmouseleave','onmousemove','onmouseout','onmouseover',
                'onmouseup','onmousewheel','onoffline','ononline',
                'onpagehide','onpagereveal','onpageshow','onpageswap',
                'onpause','onplay','onplaying','onpointercancel',
                'onpointerdown','onpointerenter','onpointerleave','onpointermove',
                'onpointerout','onpointerover','onpointerrawupdate','onpointerup','onpopstate',
                'onprogress','onratechange','onrejectionhandled','onreset',
                'onresize','onscroll','onscrollend','onscrollsnapchange',
                'onscrollsnapchanging','onsearch','onsecuritypolicyviolation','onseeked',
                'onseeking','onselect','onselectionchange','onselectstart',
                'onslotchange','onstalled','onstorage','onsubmit',
                'onsuspend','ontimeupdate','ontoggle','ontransitioncancel',
                'ontransitionend','ontransitionrun','ontransitionstart','onunhandledrejection',
                'onunload','onvolumechange','onwaiting','onwebkitanimationend',
                'onwebkitanimationiteration','onwebkitanimationstart','onwebkittransitionend','onwheel',
            ];
            for (const _oh of _onHandlers) {
                try { _sp(_oh, null); } catch (_) {}
            }

            // Never copy arbitrary parent-window own properties. Real child
            // realms expose the same Web API surface through their own global,
            // but page-authored globals remain isolated. The explicit API and
            // event-handler allowlists above/below are the only supported bridge.

            // devicePixelRatio: define as a native-tagged accessor so that
            // A script inspecting these sees both a proper descriptor (getter:fn,
            // not data) AND [native code] from Function.prototype.toString.
            // The eval runs inside the child realm so Symbol.for resolves via
            // the isolate-level global symbol registry (same symbol as parent).
            const _dprVal = globalThis.devicePixelRatio || 1;
            try {
                ops.op_eval_in_child_realm(_realmId,
                    `(function(){var _nt=Symbol.for('__browser_oxide_native__');var _g=function(){return ${_dprVal};};Object.defineProperty(_g,_nt,{value:'get devicePixelRatio',configurable:true});Object.defineProperty(_g,'name',{value:'get devicePixelRatio',configurable:true});var _s=function(v){Object.defineProperty(this,'devicePixelRatio',{value:v,writable:true,enumerable:true,configurable:true});};Object.defineProperty(_s,_nt,{value:'set devicePixelRatio',configurable:true});Object.defineProperty(_s,'name',{value:'set devicePixelRatio',configurable:true});Object.defineProperty(globalThis,'devicePixelRatio',{get:_g,set:_s,enumerable:true,configurable:true});})();`
                );
            } catch (_) {
                _sp("devicePixelRatio", _dprVal);
            }

            // ── iframe EventTarget + bidirectional postMessage (FP-E1) ───────
            // The child v8::Context has a genuine MessageEvent but NO
            // addEventListener/dispatchEvent: those live on the parent's
            // EventTarget/Window prototype chain, which the own-enumerable
            // blanket-copy above never reaches. So a framed document's
            // `window.addEventListener('message', …)` threw (swallowed),
            // leaving the iframe unable to receive OR answer messages. That
            // both (a) gates real iframe-based challenge flows (which load
            // the challenge in an <iframe> and postMessage with it) and (b)
            // differs from real Chrome (real iframes expose these). Install a
            // native-shaped EventTarget backed
            // by a realm-local listener registry + a `__deliverMessage` hook the
            // parent uses to post INTO the realm. `parent`/`top` identity is
            // left untouched (set to globalThis above) — replies route via the
            // delivered event's `source` (the standard postMessage pattern), so
            // no `iframe.contentWindow.parent === window` FP invariant changes.
            try {
                if (_markFrameMessageTrusted) {
                    ops.op_set_child_realm_prop(
                        _realmId,
                        "__oxideMarkTrusted",
                        _markFrameMessageTrusted,
                    );
                }
                ops.op_eval_in_child_realm(_realmId,
                    "(function(){var _nt=Symbol.for('__browser_oxide_native__');var _L=Object.create(null);var _mt=globalThis.__oxideMarkTrusted;try{delete globalThis.__oxideMarkTrusted;}catch(_){};"
                    + "function _n(fn,nm){try{Object.defineProperty(fn,'name',{value:nm,configurable:true});"
                    + "Object.defineProperty(fn,_nt,{value:nm,configurable:true});var ts=function toString(){return 'function '+nm+'() { [native code] }'};"
                    + "Object.defineProperty(ts,_nt,{value:'toString',configurable:true});Object.defineProperty(ts,'name',{value:'toString',configurable:true});"
                    + "Object.defineProperty(fn,'toString',{value:ts,configurable:true});}catch(_){}return fn;}"
                    + "function ael(type,fn){if(!(typeof fn==='function'||(fn&&typeof fn.handleEvent==='function')))return;var t=String(type);(_L[t]||(_L[t]=[])).push(fn);}"
                    + "function rel(type,fn){var a=_L[String(type)];if(a){var i=a.indexOf(fn);if(i>=0)a.splice(i,1);}}"
                    + "function de(ev){try{var t=ev&&ev.type;var a=_L[t];if(a)a.slice().forEach(function(h){try{(typeof h==='function'?h:h.handleEvent).call(globalThis,ev);}catch(_){}});"
                    + "var on=globalThis['on'+t];if(typeof on==='function'){try{on.call(globalThis,ev);}catch(_){}}}catch(_){}return true;}"
                    + "Object.defineProperty(globalThis,'addEventListener',{value:_n(ael,'addEventListener'),writable:true,configurable:true});"
                    + "Object.defineProperty(globalThis,'removeEventListener',{value:_n(rel,'removeEventListener'),writable:true,configurable:true});"
                    + "Object.defineProperty(globalThis,'dispatchEvent',{value:_n(de,'dispatchEvent'),writable:true,configurable:true});"
                    + "Object.defineProperty(globalThis,'__deliverMessage',{value:function(data,origin,source){try{var ev=new MessageEvent('message',{data:data,origin:origin||'',source:source||null});if(typeof _mt==='function')_mt(ev);de(ev);}catch(_){}},configurable:true});"
                    + "Object.defineProperty(globalThis,'__completeDocumentLifecycle',{value:function(){try{function _e(type,bubbles){var ev=new Event(type,{bubbles:!!bubbles});if(typeof _mt==='function')_mt(ev);return ev;}"
                    + "if(globalThis.document){document.readyState='interactive';try{document.dispatchEvent(_e('readystatechange',false));}catch(_){}try{document.dispatchEvent(_e('DOMContentLoaded',true));}catch(_){}document.readyState='complete';try{document.dispatchEvent(_e('readystatechange',false));}catch(_){}}"
                    + "de(_e('load',false));}catch(_){}},configurable:true});})();"
                );
            } catch (_) {}

            // child→parent reply target: a Proxy over the real parent window
            // whose ONLY override is postMessage — lands a 'message' on the MAIN
            // window with source === this iframe's contentWindow (cw), what
            // solvers assert (`event.source === iframe.contentWindow`). Exposed
            // to the framed doc as the delivered event's `source`, NOT as
            // `parent`, so the parent-identity invariant is preserved.
            // Capture the embedding document's origin before crossing into the
            // child context. Reading `location.origin` after a V8 context switch
            // can observe the srcdoc/about:blank realm and incorrectly yield
            // "null"; the parent's absolute href is stable here.
            const _topOrigin = _embeddingLocation.origin;
            // This frame's immediate parent realm (`null` = top page), set when
            // the parent's scripts inserted this iframe. Compute the immediate
            // parent origin before deriving the child's inherited srcdoc/about:
            // blank origin; using `_parentOrigin` before this block completes
            // triggers a TDZ ReferenceError and leaves the browsing context
            // unregistered.
            const _thisPublicWindow = _publicWindowProxyFor(el, cw);
            _realmPublicWindow.set(_realmId, _thisPublicWindow);
            _realmRawWindow.set(_realmId, cw);
            try { ops.op_register_child_public_window(_realmId, _thisPublicWindow); } catch (_) {}
            const _parentPublicWindow = _parentRealmId !== null
                ? (_realmPublicWindow.get(_parentRealmId) || globalThis)
                : globalThis;
            const _parentRawWindow = _parentRealmId !== null
                ? (_realmRawWindow.get(_parentRealmId) || globalThis)
                : globalThis;
            const _parentBaseUrl = _parentRealmId !== null
                ? (_realmBaseUrl.get(_parentRealmId) || _embeddingLocation.href)
                : _embeddingLocation.href;
            const _parentOriginCandidate = _xOrigin(_parentBaseUrl);
            const _parentOrigin = _parentOriginCandidate && _parentOriginCandidate !== "null"
                ? _parentOriginCandidate : _topOrigin;
            // The received event.origin is the sender (frame) origin, not the
            // targetOrigin arg — derive it from the src so origin checks pass.
            let _frameOrigin = _parentOrigin;
            try {
                const _fs = (el && el.getAttribute && el.getAttribute("src")) || (el && el.src) || "";
                const _o = _fs ? _xOrigin(_fs) : "";
                if (_o && _o !== "null") _frameOrigin = _o;
            } catch (_) {}
            // srcdoc/about:blank inherit their parent's base/origin. A concrete
            // src URL is updated below once it has been resolved.
            if (!_realmBaseUrl.has(_realmId)) _realmBaseUrl.set(_realmId, _parentBaseUrl);

            const _currentMessageSender = () => {
                let win = globalThis;
                try {
                    const candidate = ops.op_current_child_realm_window();
                    if (candidate) win = candidate;
                } catch (_) {}
                let href = _embeddingLocation.href;
                let origin = _topOrigin;
                try {
                    if (win && win.location) {
                        href = String(win.location.href || href);
                        origin = String(win.location.origin || origin);
                    }
                } catch (_) {}
                return { window: win, href, origin };
            };
            const _normalizeTargetOrigin = (requested, actual, senderHref) => {
                let target = requested;
                if (target && typeof target === "object") target = target.targetOrigin;
                if (target === undefined || target === "/") return actual;
                if (target === "*") return "*";
                try {
                    return new URL(String(target), senderHref || _embeddingLocation.href).origin;
                } catch (_) {
                    throw new DOMException("Invalid target origin", "SyntaxError");
                }
            };
            const _targetOriginAllows = (requested, actual, senderHref) => {
                const target = _normalizeTargetOrigin(requested, actual, senderHref);
                return target === "*" || target === actual;
            };
            // Inside the child realm these are ordinary WindowProxy relation
            // values. The *public wrapper* performs the only caller-sensitive
            // identity adaptation needed for a parent realm inspecting a nested
            // child's `contentWindow.parent`; message routing is independent and
            // uses the Rust execution-realm stack.
            _sp("parent", _parentPublicWindow);
            _sp("top", globalThis);

            // parent→child: cw.postMessage(...) (and the framed doc's own
            // window.postMessage) deliver a 'message' INTO the child realm.
            // Capture the sender WindowProxy/origin at call time because the
            // actual dispatch runs later as a task, after the execution stack
            // has unwound.
            const _pm = function postMessage(msg, origin) {
                const sender = _currentMessageSender();
                if (!_targetOriginAllows(origin, _frameOrigin, sender.href)) return;
                let _dj;
                try {
                    const cloned = typeof globalThis.structuredClone === "function"
                        ? globalThis.structuredClone(msg)
                        : msg;
                    _dj = JSON.stringify(cloned === undefined ? null : cloned);
                } catch (e) {
                    throw e && e.name === "DataCloneError"
                        ? e
                        : new DOMException("The object could not be cloned.", "DataCloneError");
                }
                setTimeout(() => {
                    // Deferred delivery gate: the child realm's *initial*
                    // load runs its scripts across many event-loop turns.
                    // A message queued early (Turnstile challenges deliver
                    // their init/config message within ms of frame
                    // insertion) can land in a gap while handlers exist but
                    // the widget's internal state machine is still being
                    // assembled -- its first dispatch then throws and the
                    // widget wedges in a retry loop. Poll for the
                    // load-settled flag that `ChildIframe` sets after the
                    // lifecycle completes; after a bounded number of ticks,
                    // deliver anyway (real browsers queue on the task
                    // source, they never drop).
                    const _deliver = () => {
                    try {
                        const _oj = JSON.stringify(sender.origin);
                        try {
                            ops.op_set_child_realm_prop(
                                _realmId,
                                "__oxideFrameMessageSource",
                                sender.window,
                            );
                        } catch (_) {}
                        ops.op_eval_in_child_realm(_realmId,
                            "try{globalThis.__deliverMessage((" + _dj + ")," + _oj + ",(globalThis.__oxideFrameMessageSource||null));}catch(_){}finally{try{delete globalThis.__oxideFrameMessageSource;}catch(_){}}"
                        );
                        try { ops.op_delete_child_realm_prop(_realmId, "__oxideFrameMessageSource"); } catch (_) {}
                    } catch (e) {
                        if (globalThis.__browser_oxide_debug) console.error("postMessage->child", e);
                    }
                    };
                    let _tries = 0;
                    const _pump = () => {
                        let ready = true;
                        try {
                            ready = ops.op_child_realm_has_property(_realmId, "__oxFrameReady") ||
                                (++_tries > 150);
                        } catch (_) { ready = true; }
                        if (ready) { _deliver(); } else { setTimeout(_pump, 4); }
                    };
                    _pump();
                });
            };
            _sp("postMessage", _pm);
            // `op_set_child_realm_prop` writes the inner global for code running
            // inside the iframe. Parent-side `contentWindow.postMessage`,
            // however, reads the outer WindowProxy dictionary; install the
            // same routing closure there as well so it does not fall through to
            // the child realm's generic same-window implementation (which would
            // report the srcdoc origin as "null").
            try {
                Object.defineProperty(cw, "postMessage", {
                    value: _pm,
                    writable: true,
                    configurable: true,
                });
            } catch (_) {
                try { cw.postMessage = _pm; } catch (_) {}
            }

            // Navigator: fresh instance proxying parent values.
            try {
                const _parentNav = globalThis.navigator;
                const _nav = Object.create(Object.prototype);
                for (const _k of [
                    "userAgent", "platform", "language", "languages",
                    "hardwareConcurrency", "deviceMemory", "maxTouchPoints",
                    "vendor", "vendorSub", "product", "productSub",
                    "appName", "appVersion", "appCodeName", "cookieEnabled",
                    "onLine", "doNotTrack", "pdfViewerEnabled",
                    "plugins", "mimeTypes",
                ]) {
                    try {
                        const _v = _parentNav[_k];
                        if (_v !== undefined) Object.defineProperty(_nav, _k, { value: _v, writable: true, configurable: true, enumerable: true });
                    } catch (_) {}
                }
                // webdriver: `false` in modern Chrome (property present,
                // value false; `undefined` would differ from real Chrome).
                // Some scripts check cw.navigator.webdriver; false is the
                // Chrome-faithful value.
                Object.defineProperty(_nav, 'webdriver', { value: false, writable: true, configurable: true, enumerable: true });
                _sp("navigator", _nav);
            } catch (_) {}

            // Own realm `fetch` — distinct reference (cw.fetch !== parent.fetch)
            try {
                const _ifetch = function fetch(...a) { return globalThis.fetch.apply(this, a); };
                Object.defineProperty(_ifetch, "name", { value: "fetch", configurable: true });
                Object.defineProperty(_ifetch, "length", { value: 1, configurable: true });
                Object.defineProperty(_ifetch, _NATIVE_TAG_SYMBOL, { value: "fetch", configurable: true });
                _sp("fetch", _ifetch);
            } catch (_) {}

            // Window host functions are not V8 intrinsics. Build each callable
            // inside the child realm (rather than aliasing a parent-realm
            // Function object) and delegate the host behavior to the tested
            // top-level implementation. This covers timers/rAF, base64 codecs,
            // microtask helpers and common Window utility functions used by
            // challenge/widget runtimes.
            const _childHostFunctions = [
                "setTimeout", "clearTimeout", "setInterval", "clearInterval",
                "requestAnimationFrame", "cancelAnimationFrame",
                "requestIdleCallback", "cancelIdleCallback",
                "queueMicrotask", "structuredClone", "reportError",
                "atob", "btoa", "getComputedStyle", "matchMedia",
            ];
            for (const _hostFnName of _childHostFunctions) {
                try {
                    const _hostSource = globalThis[_hostFnName];
                    if (typeof _hostSource !== "function") continue;
                    const _temp = "__oxideHostSource_" + _hostFnName;
                    const _hostLength = Number.isFinite(_hostSource.length)
                        ? _hostSource.length : 0;
                    _sp(_temp, _hostSource);
                    ops.op_eval_in_child_realm(
                        _realmId,
                        `(function(){var _impl=globalThis[${JSON.stringify(_temp)}];` +
                        `var _nt=Symbol.for('__browser_oxide_native__');` +
                        `var _f=function(...args){return Reflect.apply(_impl,globalThis,args);};` +
                        `Object.defineProperty(_f,'name',{value:${JSON.stringify(_hostFnName)},configurable:true});` +
                        `Object.defineProperty(_f,'length',{value:${JSON.stringify(_hostLength)},configurable:true});` +
                        `Object.defineProperty(_f,_nt,{value:${JSON.stringify(_hostFnName)},configurable:true});` +
                        `Object.defineProperty(globalThis,${JSON.stringify(_hostFnName)},{value:_f,writable:true,configurable:true});` +
                        `try{delete globalThis[${JSON.stringify(_temp)}];}catch(_){}` +
                        `})();`
                    );
                    try { ops.op_delete_child_realm_prop(_realmId, _temp); } catch (_) {}
                } catch (_) {}
            }

            // Copy key browser APIs that some scripts read from the child realm.
            // e.g. reading MediaSource.isTypeSupported from inside the child realm.
            const _apisToCopy = [
                'MediaSource', 'MediaSourceHandle', 'MediaCapabilities',
                'MediaRecorder', 'MediaStream', 'MediaStreamTrack',
                'HTMLVideoElement', 'HTMLAudioElement', 'HTMLMediaElement',
                'AudioContext', 'OfflineAudioContext',
                'RTCPeerConnection', 'RTCDataChannel',
                'Blob', 'File', 'FileReader',
                'URL', 'URLSearchParams',
                'WebSocket', 'Worker',
                'CSS', 'crypto', 'performance',
                'crossOriginIsolated', 'isSecureContext', 'origin',
                'CustomEvent', 'Event', 'EventTarget',
                'PromiseRejectionEvent', 'ErrorEvent',
                'MessageChannel', 'MessagePort', 'MessageEvent',
                'MutationObserver', 'IntersectionObserver', 'ResizeObserver',
                'PerformanceObserver',
                'TextEncoder', 'TextDecoder',
                'AbortController', 'AbortSignal',
                'ReadableStream', 'WritableStream', 'TransformStream',
                'Request', 'Response', 'Headers', 'FormData',
                'XMLHttpRequest', 'DOMParser',
                'Node', 'Element', 'Document',
                'HTMLElement', 'DocumentFragment',
                'Notification',
                // Singleton constructors the npc/crs probes expect in child realm.
                'Navigator', 'Location', 'History', 'Screen',
                'Performance', 'Permissions', 'ScreenOrientation',
                // The canvas/graphics constructor surface. Without these,
                // an iframe child realm has `CanvasRenderingContext2D ===
                // undefined` (all ctx2d proto methods missing on the child
                // realm). A script that fetches such a constructor/method
                // from the child realm gets `undefined` and then accessing a
                // property on it throws `TypeError: Cannot read properties of
                // undefined`, which differs from real Chrome. Real Chrome
                // iframe realms expose the full set. Only names that are
                // genuine main-realm globals are copied (the loop skips
                // `undefined`), so this is Chrome-faithful, not a stub.
                'CanvasRenderingContext2D', 'HTMLCanvasElement',
                'OffscreenCanvas', 'ImageData', 'Path2D', 'ImageBitmap',
                'WebGLRenderingContext', 'WebGL2RenderingContext',
                'DOMMatrix', 'DOMMatrixReadOnly', 'DOMPoint',
                'DOMRect', 'DOMRectReadOnly',
            ];
            for (const _ak of _apisToCopy) {
                try {
                    const _v = globalThis[_ak];
                    if (_v !== undefined) _sp(_ak, _v);
                } catch (_) {}
            }

            // Give realm-local DOM mutation wrappers a private callback that
            // keeps window.length/window[n] synchronized with nested iframes.
            // The child bootstrap captures it into closure state and deletes
            // the temporary global immediately.
            _sp("__oxideFrameRegistryHook", function() {
                _syncFrameRegistryForRealm(_realmId);
            });

            // Replace copied DOM/Web-interface aliases with constructors and
            // prototypes created inside this child V8 context.
            _installChildRealmInterfaces(_realmId);
            try { ops.op_delete_child_realm_prop(_realmId, "__oxideFrameRegistryHook"); } catch (_) {}
            try {
                const retarget = cw.__oxideDomRetargetValue;
                if (typeof retarget === "function") _retargetDomValue = retarget;
            } catch (_) {}
            try { ops.op_delete_child_realm_prop(_realmId, "__oxideDomRetargetValue"); } catch (_) {}
            // The document shell and its roots were created before the child
            // constructors existed; retarget them now and expose the stable
            // public WindowProxy as defaultView.
            try { _retargetDomValue(iframeDoc, "Document", true); } catch (_) {}
            // These are document-role objects even when the detached backing
            // subtree uses temporary generic elements (e.g. a DIV root for a
            // fragmentary srcdoc). Their browser-visible prototypes must follow
            // the Document contract, not the temporary backing tag.
            try { if (_docEl) _retargetDomValue(_docEl, "HTMLHtmlElement", true); } catch (_) {}
            try { if (_head) _retargetDomValue(_head, "HTMLHeadElement", true); } catch (_) {}
            try { if (_body) _retargetDomValue(_body, "HTMLBodyElement", true); } catch (_) {}
            try {
                ops.op_eval_in_child_realm(_realmId,
                    "try{Object.defineProperty(Window.prototype,Symbol.toStringTag,{value:'Window',configurable:true});}catch(_){}"
                );
            } catch (_) {}
            try { iframeDoc.defaultView = _publicWindowProxyFor(el, cw); } catch (_) {}

            // Some scripts read MediaSource.isTypeSupported from inside the
            // child realm. Wrap in IIFE to prevent __kms leaking into child realm globals
            // (some scripts detect unexpected global variables).
            // globalThis.X = Y inside an IIFE IS visible to subsequent op_eval_in_child_realm
            // calls because they all run in the same child v8::Context.
            try {
                ops.op_eval_in_child_realm(_realmId,
                    '(function(){\n' +
                    'var __kms=new Set(["video/mp4","video/webm","audio/mp4","audio/webm",' +
                    '"audio/mpeg","audio/aac","audio/x-m4a","audio/mp3","audio/x-wav",' +
                    '"audio/ogg","audio/acc","audio/mp4;codecs=\\"mp4a.40.2\\"",' +
                    '"video/mp4;codecs=\\"avc1.42E01E,mp4a.40.2\\"",' +
                    '"video/webm;codecs=\\"vp9\\""]);\n' +
                    'var _its=function isTypeSupported(t){if(typeof t!=="string")return false;var b=t.split(";")[0].trim();return __kms.has(t)||__kms.has(b);};\n' +
                    'if(typeof MediaSource==="undefined"||MediaSource===undefined){\n' +
                    'globalThis.MediaSource=function MediaSource(){throw new TypeError("Failed to construct \'MediaSource\': Illegal constructor");};\n' +
                    '}\n' +
                    'if(typeof MediaSource.isTypeSupported!=="function") MediaSource.isTypeSupported=_its;\n' +
                    'if(typeof MediaRecorder==="undefined"||MediaRecorder===undefined){\n' +
                    'globalThis.MediaRecorder=function MediaRecorder(){throw new TypeError("Failed to construct \'MediaRecorder\': Illegal constructor");};\n' +
                    '}\n' +
                    'if(typeof MediaRecorder.isTypeSupported!=="function") MediaRecorder.isTypeSupported=_its;\n' +
                    '})();\n'
                );
            } catch (_) {}

            // Align child realm globals with main window so the realms don't diverge.
            // Chrome without COOP/COEP: SharedArrayBuffer is disabled in all frames.
            // Our V8 child context natively has SAB; delete it to match.
            try {
                ops.op_eval_in_child_realm(_realmId,
                    'if(typeof SharedArrayBuffer!=="undefined"&&typeof globalThis.SharedArrayBuffer!=="undefined")' +
                    '{try{delete globalThis.SharedArrayBuffer;}catch(_){globalThis.SharedArrayBuffer=undefined;}}'
                );
            } catch (_) {}

            // Mark this the current materializing realm so any iframe its scripts
            // insert is tagged with this realm as parent. Restored after src exec.
            const _prevMatRealm = _currentMatRealmId;
            _currentMatRealmId = _realmId;

            // Execute srcdoc scripts in the child realm.
            // Some scripts inject content via srcdoc to
            // run code inside the iframe. A real browser executes those
            // scripts; we extract and eval them in the child realm context.
            if (_srcdoc) {
                try {
                    const _scriptRe = /<script[^>]*>([\s\S]*?)<\/script>/gi;
                    let _m;
                    while ((_m = _scriptRe.exec(_srcdoc)) !== null) {
                        const _src = _m[1];
                        if (_src && _src.trim()) {
                            try { ops.op_eval_in_child_realm(_realmId, _src); } catch (_) {}
                        }
                    }
                } catch (_) {}
            }

            // ── Same-origin src document: fetch + execute ────────
            // Real iframe-based challenge flows
            // point the iframe at a same-origin URL whose document
            // runs the challenge and postMessages the result to the parent.
            // Cross-origin src already returned a SecurityError proxy above, so
            // any src reaching here is same-origin. Fetch the doc, reflect its
            // URL into the child realm's location (challenge scripts read
            // location.search for ?parentOrigin=…), and execute its scripts in
            // document order. Bounded + best-effort: a failed/slow fetch is
            // swallowed and the (empty) realm is returned — never hangs the nav.
            let _iSrcUrl2 = "";
            try {
                const _rawSrc2 = (el && typeof el.getAttribute === "function")
                    ? (el.getAttribute("src") || el.src || "") : (el && el.src || "");
                if (_rawSrc2 && _rawSrc2 !== "about:blank"
                    && !/^javascript:/i.test(_rawSrc2) && !/^data:/i.test(_rawSrc2)) {
                    try { _iSrcUrl2 = new URL(_rawSrc2, (globalThis.location && globalThis.location.href) || undefined).href; }
                    catch (_) { _iSrcUrl2 = _rawSrc2; }
                }
            } catch (_) {}
            if (_iSrcUrl2) { try { _realmBaseUrl.set(_realmId, _iSrcUrl2); } catch (_) {} }
            if (_iSrcUrl2 && _ifMatDepth < _IF_MAT_MAX && !_ifMatInProgress.has(_iSrcUrl2)) {
                _ifMatDepth++; _ifMatInProgress.add(_iSrcUrl2);
                try {
                    let _u2 = null;
                    try { _u2 = new URL(_iSrcUrl2); } catch (_) {}
                    if (_u2) {
                        _sp("location", {
                            href: _u2.href, origin: _u2.origin, pathname: _u2.pathname,
                            search: _u2.search, hash: _u2.hash, host: _u2.host,
                            hostname: _u2.hostname, port: _u2.port, protocol: _u2.protocol,
                            assign() {}, replace() {}, reload() {},
                            toString() { return _u2.href; },
                        });
                    }
                    const _docHtml = ops.op_net_fetch_frame_sync(_iSrcUrl2, (globalThis.location && globalThis.location.href) || "");
                    if (_docHtml && typeof _docHtml === "string" && _docHtml.length < 5000000) {
                        const _tagRe = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
                        let _sm;
                        let _guard = 0;
                        while ((_sm = _tagRe.exec(_docHtml)) !== null && _guard++ < 64) {
                            const _attrs = _sm[1] || "";
                            const _inline = _sm[2] || "";
                            const _typeM = /\btype\s*=\s*["']?([^"'\s>]+)/i.exec(_attrs);
                            const _ty = _typeM ? _typeM[1].toLowerCase() : "";
                            if (_ty && _ty !== "text/javascript" && _ty !== "application/javascript" && _ty !== "module") continue;
                            const _srcM = /\bsrc\s*=\s*["']([^"']+)["']/i.exec(_attrs);
                            if (_srcM) {
                                let _eu = _srcM[1];
                                try { _eu = new URL(_eu, _iSrcUrl2).href; } catch (_) {}
                                try {
                                    const _code = ops.op_net_fetch_frame_sync(_eu, _iSrcUrl2);
                                    if (_code && typeof _code === "string") {
                                        try { ops.op_eval_in_child_realm(_realmId, _code); } catch (_) {}
                                    }
                                } catch (_) {}
                            } else if (_inline && _inline.trim()) {
                                try { ops.op_eval_in_child_realm(_realmId, _inline); } catch (_) {}
                            }
                        }
                    }
                } catch (_) {} finally { _ifMatInProgress.delete(_iSrcUrl2); _ifMatDepth--; }
            }

            // Browser document lifecycle runs after parser/external/inline
            // scripts have completed. The realm-local dispatcher creates child
            // Event objects, marks them trusted, updates readyState in order and
            // invokes both document and window listeners.
            try {
                ops.op_eval_in_child_realm(
                    _realmId,
                    "try{globalThis.__completeDocumentLifecycle&&globalThis.__completeDocumentLifecycle();}finally{try{delete globalThis.__completeDocumentLifecycle;}catch(_){}}",
                );
            } catch (_) {}

            _currentMatRealmId = _prevMatRealm;

            Object.assign(state, {
                _parentPublicWindow,
                _parentRawWindow,
                _topPublicWindow: globalThis,
                _initializing: false,
            });
            _setIframeState(el, state);
            _registerFrame(cw, el);
            return cw;
        }

        // ── FALLBACK: Proxy-based approach (if op unavailable) ───────────
        // Keeps existing behaviour when op_create_child_realm is not accessible
        // (e.g. worker runtime that doesn't load dom_extension).
        const remoteRealm = _buildRemoteRealm();
        const iframeLocals = {
            document: iframeDoc,
            location: { href: "about:blank" },
            parent: globalThis,
            top: globalThis,
            self: null,
            frames: [],
            screen: _iframeScreen,
            innerWidth:  globalThis.innerWidth  || 1920,
            innerHeight: globalThis.innerHeight || 1080,
            outerWidth:  globalThis.outerWidth  || 1920,
            outerHeight: globalThis.outerHeight || 1080,
            scrollX: 0, scrollY: 0, pageXOffset: 0, pageYOffset: 0,
            postMessage(msg, origin) {
                Promise.resolve().then(() => {
                    globalThis.dispatchEvent(new MessageEvent("message", { data: msg, origin: origin || "" }));
                });
            },
        };
        try {
            if (remoteRealm.Window && remoteRealm.Window.prototype) {
                Object.setPrototypeOf(iframeLocals, remoteRealm.Window.prototype);
            }
        } catch (_) {}
        try {
            const _ifetch = function fetch(...a) { return globalThis.fetch.apply(this, a); };
            Object.defineProperty(_ifetch, "name", { value: "fetch", configurable: true });
            Object.defineProperty(_ifetch, "length", { value: 1, configurable: true });
            Object.defineProperty(_ifetch, _NATIVE_TAG_SYMBOL, { value: "fetch", configurable: true });
            iframeLocals.fetch = _ifetch;
        } catch (_) {}
        try {
            const _dg = function () { return globalThis.devicePixelRatio || 1; };
            const _ds = function(v) {
                Object.defineProperty(iframeLocals, "devicePixelRatio", {
                    value: v, writable: true, enumerable: true, configurable: true,
                });
            };
            Object.defineProperty(_dg, _NATIVE_TAG_SYMBOL, { value: "get devicePixelRatio", configurable: true });
            Object.defineProperty(_dg, "name", { value: "get devicePixelRatio", configurable: true });
            Object.defineProperty(_ds, _NATIVE_TAG_SYMBOL, { value: "set devicePixelRatio", configurable: true });
            Object.defineProperty(_ds, "name", { value: "set devicePixelRatio", configurable: true });
            Object.defineProperty(iframeLocals, "devicePixelRatio", {
                get: _dg, set: _ds, enumerable: true, configurable: true,
            });
        } catch (_) {}
        const iframeWindow = new Proxy(iframeLocals, {
            get(target, prop) {
                if (prop in target) return target[prop];
                if (typeof prop === "string" && prop in remoteRealm) return remoteRealm[prop];
                try { return globalThis[prop]; } catch { return undefined; }
            },
            has(target, prop) {
                return prop in target || prop in remoteRealm || prop in globalThis;
            },
            getOwnPropertyDescriptor(target, prop) {
                if (prop in target) {
                    return Object.getOwnPropertyDescriptor(target, prop);
                }
                if (typeof prop === "string" && prop in remoteRealm) {
                    return { value: remoteRealm[prop], writable: true, enumerable: true, configurable: true };
                }
                return undefined;
            },
        });
        iframeLocals.self = iframeWindow;
        iframeLocals.window = iframeWindow;
        iframeLocals.globalThis = iframeWindow;
        iframeLocals.frames = iframeWindow;
        iframeLocals.length = 0;
        state = { contentWindow: iframeWindow, contentDocument: iframeDoc, _src: ((el && el.getAttribute && el.getAttribute("src")) || (el && el.src) || "") };
        _setIframeState(el, state);
        _registerFrame(iframeWindow, el);
        return iframeWindow;
    }
    function _getIframeDocument(el) {
        _getIframeWindow(el); // ensure state is built
        const state = _getIframeState(el);
        return state && state.contentDocument;
    }

    // Install on HTMLIFrameElement.prototype — covers parsed AND created iframes.
    if (typeof HTMLIFrameElement !== 'undefined') {
        // Frame-tree cross-frame postMessage: the engine's FrameManager drives
        // __oxFrameSetup / __oxRegisterChildFrame / __pumpFrameMessages per frame.
        globalThis.__frameIdForNode = globalThis.__frameIdForNode || {};
        globalThis.__frameOriginForNode = globalThis.__frameOriginForNode || {};
        globalThis.__frameNodeForId = globalThis.__frameNodeForId || {};
        const _frameHandleCache = {};
        function _frameHandle(fid) {
            if (fid === undefined || fid === null) return null;
            // Stable identity per frame id: pages compare `event.source ===
            // iframe.contentWindow`, so both must resolve to the same object.
            if (_frameHandleCache[fid]) return _frameHandleCache[fid];
            const _h = {
                __isFrameHandle: true,
                __frameId: fid,
                postMessage: function(msg, _targetOrigin, _transfer) {
                    if (_closedFrameIds.has(fid)) return;
                    try {
                        let _requestedOrigin = _targetOrigin;
                        let _transferList = _transfer;
                        if (_targetOrigin && typeof _targetOrigin === "object") {
                            _requestedOrigin = _targetOrigin.targetOrigin;
                            _transferList = _targetOrigin.transfer;
                        }
                        const _origin = (globalThis.location && globalThis.location.origin) || "null";
                        if (_requestedOrigin === undefined || _requestedOrigin === "/") {
                            _requestedOrigin = _origin;
                        }
                        if (globalThis.__OX_FT_MSGDBG) {
                            const _t = _transferList;
                            (globalThis.__OXPOSTS || (globalThis.__OXPOSTS = [])).push(
                                "->" + fid + " transfer=" + (_t && _t.length ? _t.length : 0)
                                + " keys=" + (msg && typeof msg === 'object' ? Object.keys(msg).slice(0, 4).join(',') : typeof msg));
                        }
                        const _s = (_browser_oxide && _browser_oxide.serializeForWire)
                            ? _browser_oxide.serializeForWire(msg) : msg;
                        // event.origin on the receiving side is the sender's origin
                        // (spec), not the targetOrigin arg; frames check it.
                        ops.op_frame_post_message(
                            fid, globalThis.__frameId || 0,
                            JSON.stringify(_s),
                            _origin,
                            String(_requestedOrigin));
                    } catch (_) {}
                },
                get closed() { return _closedFrameIds.has(fid); },
                get length() { return 0; },
            };
            _frameHandleCache[fid] = _h;
            return _h;
        }

        // Stable WindowProxy per *browsing-context generation*.
        //
        // Navigation keeps a generation and retargets its backend, so
        // `iframe.contentWindow` identity stays stable. Removing an iframe
        // destroys that child navigable; reinserting the same DOM element must
        // create a fresh WindowProxy while old references stay permanently
        // detached/closed. Wrapper identity is not stable, so NodeId owns the
        // active record and WeakMap is only a fast path.
        const _iframeWindowProxyCache = new WeakMap(); // wrapper -> record
        const _iframeWindowProxyByNode = new Map();    // NodeId -> active record
        function _iframeBackend(el) {
            try {
                // A disconnected iframe has no child navigable. Never recreate
                // a srcdoc/about:blank realm merely because an old WindowProxy
                // is read after removal; that old browsing context is closed.
                if (!el || !el.isConnected) return null;
                // Same-isolate browsing contexts keep their real Window/document
                // backend in _iframeState. Prefer it over a stale frame-tree
                // handle: nested same-realm iframe creation can happen while a
                // parent frame still has a node->frame id entry from an earlier
                // materialization path. A WindowProxy must follow the current
                // browsing context, not the historical backend.
                const localState = _getIframeState(el);
                if (localState && (localState.contentBackend || localState.contentWindow)) {
                    return localState.contentBackend || localState.contentWindow;
                }
                // srcdoc/about:blank contexts are same-isolate documents even
                // when a frame-tree entry was activated later. Do not replace
                // their document-capable Window with a network FrameHandle.
                try {
                    const srcdoc = el && el.getAttribute && el.getAttribute("srcdoc");
                    if (srcdoc !== null && srcdoc !== undefined) return _getIframeWindow(el);
                } catch (_) {}
                const nodeId = _getNodeId(el);
                const fid = globalThis.__frameIdForNode[nodeId];
                if (fid !== undefined) return _frameHandle(fid);
                return _getIframeWindow(el);
            } catch (_) {
                return null;
            }
        }
        function _iframeWindowProxy(el) {
            let record = null;
            try { record = _iframeWindowProxyCache.get(el) || null; } catch (_) {}
            const nodeId = _iframeNodeId(el);
            if (record && record.detached) record = null;
            if (!record && nodeId !== null) record = _iframeWindowProxyByNode.get(nodeId) || null;
            if (record && !record.detached) {
                try { _iframeWindowProxyCache.set(el, record); } catch (_) {}
                return record.proxy;
            }

            record = {
                el,
                nodeId,
                proxy: null,
                detached: false,
                detachedBackend: null,
                detachedDocument: null,
            };
            let proxy = null;
            const target = Object.create(null);
            const activeRealmId = () => {
                if (record.detached) return null;
                const state = _getIframeState(record.el);
                return state && typeof state._realmId === "number" ? state._realmId : null;
            };
            const currentBackend = () => record.detached
                ? record.detachedBackend
                : _iframeBackend(record.el);
            const handler = {
                get(_target, prop) {
                    if (prop === "window" || prop === "self" || prop === "globalThis" || prop === "frames") {
                        return proxy;
                    }
                    if (prop === Symbol.toStringTag) return "Window";
                    if (record.detached) {
                        if (prop === "closed") return true;
                        if (prop === "parent" || prop === "top" || prop === "frameElement") return null;
                        if (prop === "document") return record.detachedDocument;
                    }
                    const state = _getIframeState(record.el);
                    if (state && prop === "parent") {
                        try {
                            const callerRealm = ops.op_current_child_realm_id();
                            if (typeof state._parentRealmId === "number"
                                && callerRealm === state._parentRealmId
                                && state._parentRawWindow) {
                                return state._parentRawWindow;
                            }
                        } catch (_) {}
                        if (state._parentPublicWindow) return state._parentPublicWindow;
                    }
                    if (state && prop === "top" && state._topPublicWindow) {
                        return state._topPublicWindow;
                    }
                    const realmId = activeRealmId();
                    if (realmId !== null && typeof prop === "string") {
                        try {
                            const value = ops.op_child_realm_get_property(realmId, prop);
                            if (prop === "closed" && value === undefined) return false;
                            return value;
                        } catch (_) {}
                    }
                    const backend = currentBackend();
                    if (!backend) return prop === "closed" ? true : undefined;
                    // A same-isolate iframe may have been observed through a
                    // frame-tree path before its Window state was populated.
                    // Preserve the browsing-context API surface (document,
                    // location, parent) by recovering the live backend instead
                    // of exposing a bare frame transport handle.
                    if (!record.detached && prop === "document" && !backend.document) {
                        try {
                            const live = _getIframeWindow(record.el);
                            if (live && live.document) return live.document;
                        } catch (_) {}
                    }
                    try { return Reflect.get(backend, prop, backend); } catch (_) { return undefined; }
                },
                set(_target, prop, value) {
                    const realmId = activeRealmId();
                    if (realmId !== null && typeof prop === "string") {
                        try { return !!ops.op_child_realm_set_property(realmId, prop, value); } catch (_) { return false; }
                    }
                    const backend = currentBackend();
                    if (!backend) return false;
                    try { return Reflect.set(backend, prop, value, backend); } catch (_) { return false; }
                },
                has(_target, prop) {
                    if (prop === "window" || prop === "self" || prop === "globalThis" || prop === "frames") return true;
                    if (record.detached && (prop === "closed" || prop === "parent" || prop === "top" || prop === "frameElement" || prop === "document")) return true;
                    const realmId = activeRealmId();
                    if (realmId !== null && typeof prop === "string") {
                        try { return !!ops.op_child_realm_has_property(realmId, prop); } catch (_) { return false; }
                    }
                    const backend = currentBackend();
                    try { return !!backend && Reflect.has(backend, prop); } catch (_) { return false; }
                },
                ownKeys() {
                    const realmId = activeRealmId();
                    if (realmId !== null) {
                        try {
                            const keys = Array.from(ops.op_child_realm_own_property_names(realmId) || []);
                            return Array.from(new Set(keys.map(String)));
                        } catch (_) {}
                    }
                    const backend = currentBackend();
                    try { return backend ? Reflect.ownKeys(backend) : []; } catch (_) { return []; }
                },
                getOwnPropertyDescriptor(_target, prop) {
                    if (prop === "window" || prop === "self" || prop === "globalThis" || prop === "frames") {
                        return { value: proxy, writable: true, enumerable: true, configurable: true };
                    }
                    const realmId = activeRealmId();
                    if (realmId !== null && typeof prop === "string") {
                        try {
                            const desc = ops.op_child_realm_get_own_property_descriptor(realmId, prop);
                            if (!desc || typeof desc !== "object") return undefined;
                            const copy = Object.assign({}, desc);
                            copy.configurable = true;
                            return copy;
                        } catch (_) { return undefined; }
                    }
                    const backend = currentBackend();
                    try {
                        const desc = backend && Reflect.getOwnPropertyDescriptor(backend, prop);
                        if (!desc) return undefined;
                        const copy = Object.assign({}, desc);
                        copy.configurable = true;
                        return copy;
                    } catch (_) { return undefined; }
                },
                defineProperty(_target, prop, desc) {
                    // Reporting success for a non-configurable descriptor that
                    // does not exist on the stable proxy target violates the
                    // ECMAScript Proxy invariants. Reject it explicitly rather
                    // than letting a later reflection operation crash.
                    if (desc && desc.configurable === false) return false;
                    const realmId = activeRealmId();
                    if (realmId !== null && typeof prop === "string") {
                        try { return !!ops.op_child_realm_define_property(realmId, prop, desc); } catch (_) { return false; }
                    }
                    const backend = currentBackend();
                    try { return !!backend && Reflect.defineProperty(backend, prop, desc); } catch (_) { return false; }
                },
                deleteProperty(_target, prop) {
                    const realmId = activeRealmId();
                    if (realmId !== null && typeof prop === "string") {
                        try { return !!ops.op_child_realm_delete_property(realmId, prop); } catch (_) { return false; }
                    }
                    const backend = currentBackend();
                    try { return !!backend && Reflect.deleteProperty(backend, prop); } catch (_) { return false; }
                },
                getPrototypeOf() {
                    const realmId = activeRealmId();
                    if (realmId !== null) {
                        try {
                            const WindowCtor = ops.op_child_realm_get_property(realmId, "Window");
                            if (WindowCtor && WindowCtor.prototype) return WindowCtor.prototype;
                        } catch (_) {}
                    }
                    const backend = currentBackend();
                    try { return backend ? Reflect.getPrototypeOf(backend) : Object.prototype; } catch (_) { return Object.prototype; }
                },
                setPrototypeOf(_target, proto) {
                    const backend = currentBackend();
                    try { return !!backend && Reflect.setPrototypeOf(backend, proto); } catch (_) { return false; }
                },
                isExtensible() {
                    return true;
                },
                preventExtensions() {
                    // A WindowProxy cannot become non-extensible while its
                    // backing Window changes across navigation.
                    return false;
                },
            };
            proxy = new Proxy(target, handler);
            record.proxy = proxy;
            try { _iframeWindowProxyCache.set(el, record); } catch (_) {}
            if (nodeId !== null) _iframeWindowProxyByNode.set(nodeId, record);
            return proxy;
        }
        _detachPublicWindowProxyFor = function(el, state) {
            const nodeId = _iframeNodeId(el);
            let record = null;
            try { record = _iframeWindowProxyCache.get(el) || null; } catch (_) {}
            if ((!record || record.detached) && nodeId !== null) {
                record = _iframeWindowProxyByNode.get(nodeId) || null;
            }
            if (!record || record.detached) return;

            // Freeze the observable Window own-property surface before the
            // active child realm is released. A saved WindowProxy reference
            // remains usable after iframe removal (with closed/parent/top/
            // frameElement overridden below), including page-authored values.
            // Copy descriptors rather than values so accessors and enumerability
            // retain their pre-detach shape.
            let detachedSnapshot = null;
            try {
                detachedSnapshot = Object.create(null);
                for (const key of Reflect.ownKeys(record.proxy)) {
                    const desc = Reflect.getOwnPropertyDescriptor(record.proxy, key);
                    if (!desc) continue;
                    const copy = Object.assign({}, desc);
                    copy.configurable = true;
                    try { Object.defineProperty(detachedSnapshot, key, copy); } catch (_) {}
                }
                try { Object.setPrototypeOf(detachedSnapshot, Reflect.getPrototypeOf(record.proxy)); } catch (_) {}
            } catch (_) {
                detachedSnapshot = null;
            }

            record.detached = true;
            record.detachedBackend = detachedSnapshot || (state
                ? (state.contentBackend || state.contentWindow || null)
                : null);
            record.detachedDocument = state && state.contentDocument ? state.contentDocument : null;
            if (record.detachedDocument) {
                try { record.detachedDocument.defaultView = null; } catch (_) {}
            }

            try { _iframeWindowProxyCache.delete(el); } catch (_) {}
            if (nodeId !== null && _iframeWindowProxyByNode.get(nodeId) === record) {
                _iframeWindowProxyByNode.delete(nodeId);
            }
        };
        _publicWindowProxyFor = function(el, backend) {
            try { return _iframeWindowProxy(el); } catch (_) { return backend; }
        };
        function _frameSourceHandle(fid) {
            try {
                const nodeId = globalThis.__frameNodeForId[fid];
                if (nodeId !== undefined) {
                    const el = _wrapNode(nodeId);
                    if (el) return _iframeWindowProxy(el);
                }
            } catch (_) {}
            return _frameHandle(fid);
        }
        globalThis.__frameHandleFor = _frameHandle;
        globalThis.__pumpFrameMessages = function() {
            // Deferred-delivery gate: hold queued cross-frame messages until
            // this realm's initial load has settled. The driver calls this
            // pump as soon as the mailbox is non-empty, which during page load
            // lands between this realm's per-script event-loop turns — the
            // gap that wedges Turnstile-style challenge state machines
            // (first handler dispatch throws -> crashed_retry loop).
            // `__oxFrameReady` is set by the driver / ChildIframe right
            // after the document lifecycle completes. Bounded: after 400
            // gate passes (~sweeps, not ms) deliver anyway so a realm that
            // never settles still receives its mail (browsers never drop).
            if (!globalThis.__oxFrameReady) {
                const n = (globalThis.__oxGateN = (globalThis.__oxGateN || 0) + 1);
                try {
                    const gp = globalThis.__oxGP || (globalThis.__oxGP = []);
                    if (gp.length < 8) gp.push("g" + n + "@" + Math.round(performance.now()));
                } catch (_) {}
                if (n < 400) return;
            } else {
                globalThis.__oxGateN = 0;
            }
            // Time floor: the challenge's first message handler reads lazy
            // lookup tables that later timers / phases finish building; a
            // dispatch before ~1.5s realm-age throws inside the CF VM
            // ("reading 'call' of undefined"), the widget's own wrapper then
            // declares the realm crashed and renavigates — a loop. c22
            // proved a +1.2s replay of the same message runs clean, so the
            // tables exist by then: simply hold the FIRST delivery window
            // until the realm has had 1.5s of Timer-eligible life. Leaves
            // messages queued (driver re-pumps); bounded like the gate.
            {
                const now = performance.now();
                if (globalThis.__oxT0 === undefined) globalThis.__oxT0 = now;
                const age = now - globalThis.__oxT0;
                if (age < 1500) {
                    const n2 = (globalThis.__oxGateT = (globalThis.__oxGateT || 0) + 1);
                    if (n2 < 400) {
                        try {
                            const gt = globalThis.__oxGT || (globalThis.__oxGT = []);
                            if (gt.length < 6) gt.push("t" + n2 + "@" + Math.round(now));
                        } catch (_) {}
                        return;
                    }
                }
            }
            let arr;
            try { arr = JSON.parse(ops.op_frame_take_messages(globalThis.__frameId || 0)); }
            catch (_) { return; }
            const ring = globalThis.__oxFrameMsgLog
                || (globalThis.__oxFrameMsgLog = []);
            for (const m of arr) {
                let data;
                try {
                    data = (_browser_oxide && _browser_oxide.deserializeFromWire)
                        ? _browser_oxide.deserializeFromWire(m.d) : m.d;
                } catch (_) { data = m.d; }
                let shape = typeof data;
                try {
                    if (data && typeof data === "object") {
                        shape = "object:" + Object.keys(data).slice(0, 6).join(",");
                    }
                } catch (_) { shape = "object:?"; }
                const entry = (m.t || "").slice(0, 24) + "|" + (m.o || "").slice(0, 24)
                    + "|" + shape + "|" + String(typeof data === "string" ? data.slice(0, 80) : "").slice(0, 80);
                const last = ring[ring.length - 1];
                const lastShape = last && typeof last === "string" ? last.split("|")[2] : null;
                if (lastShape === shape && shape.indexOf("event,seq") >= 0 && last.indexOf("DISPATCH") < 0) {
                    const at = last.lastIndexOf(" x");
                    const base = at > 0 ? last.slice(0, at) : last;
                    const cnt = at > 0 ? (parseInt(last.slice(at + 2), 10) || 1) : 1;
                    ring[ring.length - 1] = base + " x" + (cnt + 1);
                } else {
                    if (ring.length >= 24) ring.shift();
                    ring.push(entry);
                }
                try {
                    const ev = new MessageEvent("message", { data: data, origin: m.o || "", source: _frameSourceHandle(m.s) });
                    if (_markFrameMessageTrusted) _markFrameMessageTrusted(ev);
                    globalThis.dispatchEvent(ev);
                } catch (e) {
                    let full = "";
                    try { full = JSON.stringify(data).slice(0, 1200); } catch (_) { full = "unserializable"; }
                    ring.shift();
                    ring.push("DISPATCH_FAIL:" + (e && e.message) + "|DATA:" + full
                        + "|STACK:" + (e && e.stack ? String(e.stack).replace(/\n/g, " ~ ").slice(0, 900) : ""));
                    // Redelivery: Turnstile-style challenge bundles register
                    // their message handler before the VM string tables are
                    // finished; the first dispatch in the load window throws, and a plain drop wedges the state machine until
                    // its own crashed_retry renavigates. A single late
                    // redelivery (the handler is idempotent from a second
                    // dispatch, verified empirically) lets the same realm
                    // recover without a renavigation. Bounded and once-only
                    // per failure so a deterministically-broken handler can't
                    // spin the loop.
                    try {
                        const rd = globalThis.__oxMsgRD
                            || (globalThis.__oxMsgRD = { n: 0 });
                        if (rd.n < 24) {
                            rd.n++;
                            setTimeout(function () {
                                try {
                                    const ev2 = new MessageEvent("message", {
                                        data: data,
                                        origin: m.o || "",
                                        source: _frameSourceHandle(m.s),
                                    });
                                    if (_markFrameMessageTrusted) _markFrameMessageTrusted(ev2);
                                    globalThis.dispatchEvent(ev2);
                                    if (ring.length >= 24) ring.shift();
                                    ring.push("RD_OK@" + Math.round(performance.now()));
                                } catch (e2) {
                                    ring.push("RD_FAIL:" + (e2 && e2.message));
                                }
                            }, 1200);
                        }
                    } catch (_) {}
                }
            }
        };
        // Turnstile-style challenge code assembles large program strings at
        // runtime and runs them through the Function constructor — those
        // sources are invisible to any static dump of the page's <script>
        // text. Transparently tap the constructor and mirror big sources to
        // the parent/top window (which survives the challenge frame's
        // crashed_retry renavigation) so timeouts can dump them.
        try {
            const _NativeFunction = globalThis.Function;
            const _recordEvalSource = (src) => {
                try {
                    if (typeof src !== "string" || src.length < 512) return;
                    const log = globalThis.__oxEvalSrcLog
                        || (globalThis.__oxEvalSrcLog = []);
                    if (log.length >= 6) return;
                    for (let i = 0; i < log.length; i++) {
                        if (log[i] && log[i].length === src.length) return;
                    }
                    log.push(src);
                    const payload = {
                        __oxEvalSrc: {
                            href: String((globalThis.location && globalThis.location.href) || "").slice(-80),
                            code: src,
                        },
                    };
                    for (const w of [globalThis.parent, globalThis.top]) {
                        try {
                            if (w && w !== globalThis && typeof w.postMessage === "function") {
                                w.postMessage(payload, "*");
                            }
                        } catch (_) {}
                    }
                } catch (_) {}
            };
            // Proxy keeps the native Function identity: instanceof, .prototype,
            // .name and .length all behave exactly as before; we only observe
            // calls/constructs to mirror dynamically assembled sources.
            globalThis.Function = new Proxy(_NativeFunction, {
                apply(target, thisArg, args) {
                    try {
                        _recordEvalSource(typeof args[0] === "string" ? args[0] : (args[0] != null ? String(args[0]) : ""));
                    } catch (_) {}
                    return Reflect.apply(target, thisArg, args);
                },
                construct(target, args, newTarget) {
                    try {
                        _recordEvalSource(typeof args[0] === "string" ? args[0] : (args[0] != null ? String(args[0]) : ""));
                    } catch (_) {}
                    return Reflect.construct(target, args, newTarget);
                },
            });
        } catch (_) {}
        globalThis.__oxFrameSetup = function(frameId, parentId, topId) {
            globalThis.__frameId = frameId;
            globalThis.__parentFrameId = parentId;
            globalThis.__topFrameId = topId;
            try {
                ops.op_frame_register_origin(
                    frameId,
                    (globalThis.location && globalThis.location.origin) || "null",
                );
            } catch (_) {}
            // window.parent / window.top are getters that read these overrides.
            if (parentId !== frameId) globalThis.__frameParentOverride = _frameHandle(parentId);
            if (topId !== frameId) globalThis.__frameTopOverride = _frameHandle(topId);
        };
        globalThis.__oxRegisterChildFrame = function(nodeId, childFrameId, childOrigin) {
            _debugFrameLifecycle({phase:'register',nodeId,fid:childFrameId,origin:childOrigin||'null'});
            const previous = globalThis.__frameIdForNode[nodeId];
            if (previous !== undefined && previous !== childFrameId) {
                _closedFrameIds.add(previous);
                delete globalThis.__frameNodeForId[previous];
            }
            globalThis.__frameIdForNode[nodeId] = childFrameId;
            globalThis.__frameOriginForNode[nodeId] = childOrigin || "null";
            globalThis.__frameNodeForId[childFrameId] = nodeId;
            _closedFrameIds.delete(childFrameId);
        };
        globalThis.__oxForgetSameIsolateFrame = function(nodeId) {
            try {
                const frames = document.querySelectorAll("iframe");
                for (let i = 0; i < frames.length; i++) {
                    if (_getNodeId(frames[i]) === nodeId) {
                        _disposeIframeRealm(frames[i], true);
                        return true;
                    }
                }
            } catch (_) {}
            return false;
        };
        globalThis.__oxFrameHostConnected = function(nodeId) {
            try {
                const el = _wrapNode(nodeId);
                return !!(el && el.isConnected);
            } catch (_) {
                return false;
            }
        };
        globalThis.__oxFrameHostMatches = function(nodeId, expectedUrl) {
            try {
                const el = _wrapNode(nodeId);
                if (!el || !el.isConnected || el.hasAttribute("srcdoc")) return false;
                const raw = el.getAttribute("src") || "";
                if (!raw || raw === "about:blank" || /^javascript:/i.test(raw) || /^data:/i.test(raw)) {
                    return false;
                }
                const actual = new URL(raw, (globalThis.location && globalThis.location.href) || "about:blank").href;
                return actual === String(expectedUrl || "");
            } catch (_) {
                return false;
            }
        };
        globalThis.__oxUnregisterChildFrame = function(nodeId) {
            try {
                const fid = globalThis.__frameIdForNode[nodeId];
                _debugFrameLifecycle({phase:'unregister',nodeId,fid:fid===undefined?null:fid});
                if (fid !== undefined) _closedFrameIds.add(fid);
                delete globalThis.__frameIdForNode[nodeId];
                delete globalThis.__frameOriginForNode[nodeId];
                if (fid !== undefined) delete globalThis.__frameNodeForId[fid];
                return fid === undefined ? 0 : fid;
            } catch (_) {
                return 0;
            }
        };

        // Fire the host `<iframe>`'s `load` event once its child frame has
        // materialized: pages gate cross-frame setup on `iframe.onload`.
        globalThis.__oxFrameLoaded = function(nodeId) {
            try {
                const el = _wrapNode(nodeId);
                if (!el) return;
                const ev = new Event("load");
                if (_markFrameMessageTrusted) _markFrameMessageTrusted(ev);
                try { Object.defineProperty(ev, "target", { value: el, configurable: true }); } catch (_) {}
                try { el.dispatchEvent(ev); } catch (_) {}
            } catch (_) {}
        };

        Object.defineProperty(HTMLIFrameElement.prototype, 'contentWindow', {
            get: function() {
                if (!this.isConnected) return null;
                // Accessing contentWindow materializes the browsing context even
                // if the caller does not immediately read a property from it.
                const backend = _iframeBackend(this);
                if (!backend) return null;
                const publicWindow = _publicWindowProxyFor(this, backend);
                _registerFrame(publicWindow, this);
                return publicWindow;
            },
            configurable: true,
            enumerable: true,
        });
        Object.defineProperty(HTMLIFrameElement.prototype, 'contentDocument', {
            get: function() {
                if (!this.isConnected) return null;
                try {
                    if (globalThis.__frameId !== undefined && globalThis.__frameIdForNode) {
                        const nodeId = _getNodeId(this);
                        const fid = globalThis.__frameIdForNode[nodeId];
                        if (fid !== undefined) {
                            const childOrigin = globalThis.__frameOriginForNode[nodeId] || "null";
                            const parentOrigin = (globalThis.location && globalThis.location.origin) || "null";
                            if (childOrigin !== parentOrigin) return null;
                        }
                    }
                } catch (_) {}
                return _getIframeDocument(this);
            },
            configurable: true,
            enumerable: true,
        });
        // Attribute/IDL navigation for iframe srcdoc/src. Both surfaces must
        // converge so `setAttribute`, property assignment and attribute removal
        // cannot leave the JS WindowProxy and Rust frame tree on different
        // documents.
        const _srcdocValues = new WeakMap();
        const _iframeBaseSetAttribute = Element.prototype.setAttribute;
        const _iframeBaseRemoveAttribute = Element.prototype.removeAttribute;
        const _iframeSrcDescriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'src');
        const _activateSrcdocNavigation = (el) => {
            const nodeId = _getNodeId(el);
            const frameId = globalThis.__frameIdForNode && globalThis.__frameIdForNode[nodeId];
            const state = _getIframeState(el);
            if (frameId !== undefined) {
                try { globalThis.__oxUnregisterChildFrame(nodeId); } catch (_) {}
            }
            if (state && state._realmId !== undefined) {
                _disposeIframeRealm(el, true);
            }
            if (frameId !== undefined || state || el.isConnected) {
                try { _getIframeWindow(el); } catch (_) {}
            }
        };
        Object.defineProperty(HTMLIFrameElement.prototype, 'srcdoc', {
            get: function() {
                return _srcdocValues.has(this)
                    ? _srcdocValues.get(this)
                    : (this.getAttribute('srcdoc') || '');
            },
            set: function(v) {
                const next = String(v);
                _srcdocValues.set(this, next);
                try { _iframeBaseSetAttribute.call(this, 'srcdoc', next); } catch (_) {}
                _activateSrcdocNavigation(this);
            },
            configurable: true,
            enumerable: true,
        });
        Object.defineProperty(HTMLIFrameElement.prototype, 'setAttribute', {
            value: function(name, value) {
                const lower = String(name).toLowerCase();
                if (lower === 'srcdoc') {
                    const next = String(value);
                    _srcdocValues.set(this, next);
                    _iframeBaseSetAttribute.call(this, 'srcdoc', next);
                    _activateSrcdocNavigation(this);
                    return;
                }
                if (lower === 'src' && _iframeSrcDescriptor && _iframeSrcDescriptor.set) {
                    _iframeSrcDescriptor.set.call(this, value);
                    return;
                }
                const result = _iframeBaseSetAttribute.call(this, name, value);
                if (lower === 'name') _syncOwningFrameRegistry(this);
                return result;
            },
            writable: true,
            configurable: true,
        });
        Object.defineProperty(HTMLIFrameElement.prototype, 'removeAttribute', {
            value: function(name) {
                const lower = String(name).toLowerCase();
                const existed = this.hasAttribute(lower);
                const result = _iframeBaseRemoveAttribute.call(this, name);
                if (!existed) return result;

                if (lower === 'srcdoc') {
                    _srcdocValues.delete(this);
                    const state = _getIframeState(this);
                    if (state && state._realmId !== undefined) _disposeIframeRealm(this, true);
                    if (this.isConnected) {
                        const rawSrc = this.getAttribute('src') || '';
                        if (rawSrc && _iframeSrcDescriptor && _iframeSrcDescriptor.set) {
                            _iframeSrcDescriptor.set.call(this, rawSrc);
                        } else {
                            try { _getIframeWindow(this); } catch (_) {}
                        }
                    }
                } else if (lower === 'src' && !this.hasAttribute('srcdoc')) {
                    const nodeId = _getNodeId(this);
                    try { globalThis.__oxUnregisterChildFrame && globalThis.__oxUnregisterChildFrame(nodeId); } catch (_) {}
                    try { _disposeIframeRealm(this, true); } catch (_) {}
                    if (this.isConnected) {
                        try { _getIframeWindow(this); } catch (_) {}
                    }
                } else if (lower === 'name') {
                    _syncOwningFrameRegistry(this);
                }
                return result;
            },
            writable: true,
            configurable: true,
        });
    }

    // Keep the createElement customElements-upgrade hook — still needed for
    // user-defined custom elements.
    const _origCreateElement = Document.prototype.createElement;
    Document.prototype.createElement = function(tag) {
        const el = _origCreateElement.call(this, tag);
        const ceEntry = globalThis._customElementsRegistry && globalThis._customElementsRegistry.get(tag.toLowerCase());
        if (ceEntry) {
            Object.setPrototypeOf(el, ceEntry.constructor.prototype);
            try { ceEntry.constructor.call(el); } catch (e) { console.error(e); }
            el._ceUpgraded = true;
        }
        return el;
    };

    // ================================================================
    // Native-code mask sweep for every JS-defined Web API method.
    //
    // Without this, Function.prototype.toString called on attachShadow,
    // queueMicrotask, Document.createElement, etc. returns the literal
    // JS source — including our deno_core op names like
    // `op_dom_attach_shadow`. Real Chrome returns
    // `function NAME() { [native code] }`; without masking, scripts that
    // inspect these would see our op names and detect the difference.
    //
    // Strategy: walk every named own property of every Web API
    // prototype we define, find any function-typed values + getters +
    // setters, and apply _maskFunction. Idempotent — re-masking a
    // tagged function is a no-op.
    if (typeof globalThis._maskFunction === 'function') {
        const _mask = globalThis._maskFunction;
        const _walkProto = (ctor, ctorName) => {
            if (!ctor) return;
            try { _mask(ctor, ctorName); } catch (_) {}
            const proto = ctor.prototype;
            if (!proto) return;
            for (const key of Object.getOwnPropertyNames(proto)) {
                if (key === 'constructor') continue;
                const desc = Object.getOwnPropertyDescriptor(proto, key);
                if (!desc) continue;
                try {
                    if (typeof desc.value === 'function') _mask(desc.value, key);
                    if (typeof desc.get === 'function') _mask(desc.get, `get ${key}`);
                    if (typeof desc.set === 'function') _mask(desc.set, `set ${key}`);
                } catch (_) {}
            }
        };
        // Every JS-defined Web API class in this bootstrap, plus
        // siblings from window_bootstrap, fetch_bootstrap,
        // canvas_bootstrap, etc. Listed by name so the sweep is
        // conservative — only masks what we've verified exists.
        const _toMask = [
            'EventTarget', 'Node', 'Element', 'HTMLElement',
            'Document', 'HTMLDocument', 'DocumentFragment',
            'ShadowRoot', 'Text', 'Comment', 'Attr',
            'NodeList', 'HTMLCollection', 'NamedNodeMap',
            'DOMTokenList', 'CSSStyleDeclaration',
            // Window-bootstrap-defined classes that previously leaked
            // their JS source via Function.prototype.toString.
            'Bluetooth', 'StorageManager', 'SharedWorker',
            'WorkerGlobalScope', 'NetworkInformation', 'MediaDevices',
            'ServiceWorkerContainer', 'Permissions', 'PermissionStatus',
            'Notification', 'Clipboard', 'CredentialsContainer',
            'PresentationConnection', 'XRSystem', 'GPUAdapter',
            // Canvas/Audio
            'AudioContext', 'BaseAudioContext', 'OfflineAudioContext',
            'AudioWorkletNode', 'OscillatorNode', 'GainNode',
            'AnalyserNode', 'BiquadFilterNode', 'DynamicsCompressorNode',
            // Workers
            'Worker', 'BroadcastChannel', 'MessageChannel', 'MessagePort',
            // Media
            'MediaRecorder', 'MediaSource', 'MediaSession',
            // HTML element subclasses (mostly empty markers, but their
            // class source still leaks via toString without masking).
            'HTMLDivElement', 'HTMLSpanElement', 'HTMLParagraphElement',
            'HTMLAnchorElement', 'HTMLImageElement', 'HTMLCanvasElement',
            'HTMLScriptElement', 'HTMLStyleElement', 'HTMLLinkElement',
            'HTMLMetaElement', 'HTMLTableElement', 'HTMLIFrameElement',
            'HTMLBodyElement', 'HTMLHtmlElement', 'HTMLHeadElement',
            'HTMLInputElement', 'HTMLButtonElement', 'HTMLSelectElement',
            'HTMLTextAreaElement', 'HTMLFormElement', 'HTMLLabelElement',
            'HTMLOptionElement', 'HTMLUListElement', 'HTMLOListElement',
            'HTMLLIElement', 'HTMLHeadingElement', 'HTMLHRElement',
            'HTMLBRElement', 'HTMLPreElement', 'HTMLBlockquoteElement',
            'HTMLVideoElement', 'HTMLAudioElement', 'HTMLMediaElement',
            'HTMLSourceElement', 'HTMLTrackElement', 'HTMLPictureElement',
            'HTMLTemplateElement', 'HTMLSlotElement', 'HTMLDialogElement',
            'HTMLDetailsElement', 'HTMLProgressElement', 'HTMLMeterElement',
        ];
        for (const name of _toMask) {
            const ctor = globalThis[name];
            if (typeof ctor === 'function') _walkProto(ctor, name);
        }

        // Top-level globalThis function-typed members that should be
        // native. queueMicrotask + fetch were the worst offenders —
        // both leaked their literal JS source via
        // Function.prototype.toString.
        const _topLevelFns = [
            'queueMicrotask', 'fetch', 'setTimeout', 'clearTimeout',
            'setInterval', 'clearInterval', 'requestAnimationFrame',
            'cancelAnimationFrame', 'requestIdleCallback', 'cancelIdleCallback',
            'structuredClone', 'reportError',
            'getComputedStyle', 'matchMedia', 'scroll', 'scrollTo', 'scrollBy',
            'alert', 'confirm', 'prompt', 'open', 'close', 'focus', 'blur',
            'postMessage', 'addEventListener', 'removeEventListener',
            'dispatchEvent',
        ];
        for (const name of _topLevelFns) {
            const fn = globalThis[name];
            if (typeof fn === 'function') {
                try { _mask(fn, name); } catch (_) {}
            }
        }
    }

    // Minimal window stub
    globalThis.window = globalThis;
    globalThis.self = globalThis;

    // Expose node-id resolution to sibling bootstrap files that need it
    // (event_bootstrap.js wires listeners by nodeId, not by Node identity).
    // Installed non-enumerable; cleanup_bootstrap.js deletes __browser_oxide
    // before page scripts run. Callers must CAPTURE the helper during
    // their own bootstrap execution, not look it up per-call.
    const _internalBridge = globalThis.__browser_oxide || {};
    _internalBridge._getNodeId = _getNodeId;
    _internalBridge._wrapNode = _wrapNode;
    _internalBridge._setCurrentScript = _setCurrentScript;
    _internalBridge._setCurrentScriptById = function(nodeId) {
        _setCurrentScript(nodeId === null || nodeId === undefined ? null : _wrapNode(nodeId));
    };
    _internalBridge._installFrameMessageTrustMarker = function(fn) {
        if (typeof fn === 'function') _markFrameMessageTrusted = fn;
    };
    Object.defineProperty(globalThis, '__browser_oxide', {
        value: _internalBridge,
        enumerable: false,
        configurable: true,
        writable: false,
    });

    // Warm-reuse DOM-registry reaper. Every registry below is module-private
    // and keyed by (or holding) state that belongs to ONE document, yet it
    // lives as long as the `JsRuntime`. On the cold path that is exactly the
    // life of the page, so nothing was ever pruned; on the warm path
    // (`PagePool` / `Page::navigate_warm`) `replace_dom` swaps the document
    // underneath them and they accumulate forever. See
    // `Page::reset_for_reuse`, which calls this.
    //
    // `_nodeCache` is doubly wrong across a swap: it is keyed by `nodeId`, and
    // node IDs restart at zero for the new document, so a surviving entry
    // hands the NEW page's node the OLD page's wrapper (with the old page's
    // expandos on it). The `WeakRef` values do not save us — an old wrapper
    // stays alive as long as any listener closure references it.
    Object.defineProperty(globalThis, '__resetDomRegistries', {
        value: function __resetDomRegistries() {
            _nodeCache.clear();
            _scrollState.clear();
            _syncFetchInFlight.clear();
            // Observers registered by the previous page's scripts. Pages
            // routinely never call `disconnect()`, so this only shrinks on
            // reuse — each retained observer pins its callback closure and
            // every observed target wrapper.
            _moObservers.length = 0;
            _appendedIframes.length = 0;
            _frameRegistry.length = 0;
            try { globalThis.__ifAppendCount = 0; } catch (_) {}
            // Re-seed the document wrapper: `_wrapNode` must keep returning
            // the singleton `_document` for the document node id, which
            // `replace_dom` preserves.
            try { _nodeCache.set(ops.op_dom_document_node(), new WeakRef(_document)); } catch (_) {}
            try { globalThis.__oxInstallEvalTap(); } catch (_) {}
        },
        writable: true,
        configurable: true,
        enumerable: false,
    });

    // Runtime-assembled program sources (new Function / indirect eval) are
    // invisible to any static <script> dump, yet crash stacks point into
    // them ('call' at <anonymous>:7:18868 in the Turnstile challenge). The
    // recorder is a plain Proxy over the realm's own Function: identity
    // semantics (instanceof, prototype, name, length) are untouched, we
    // only mirror big sources into __oxEvalSrcLog for the probe to dump.
    // NOT wrapping direct `eval(` — a wrapper would break its
    // scope-capture semantics, and the Function constructor covers the
    // crash sites we chase.
    globalThis.__oxInstallEvalTap = function __oxInstallEvalTap() {
        if (globalThis.__oxEvalTapReady) return;
        globalThis.__oxEvalTapReady = true;
        const log = globalThis.__oxEvalSrcLog
            || (globalThis.__oxEvalSrcLog = []);
        const record = (src) => {
            try {
                globalThis.__oxTapN = (globalThis.__oxTapN | 0) + 1;
                if (typeof src !== "string" || src.length < 512) return;
                if (log.length >= 8) return;
                for (let i = 0; i < log.length; i++) {
                    if (log[i] && log[i].length === src.length) return;
                }
                log.push(src);
            } catch (_) {}
        };
        try {
            const NativeFunction = globalThis.Function;
            globalThis.Function = new Proxy(NativeFunction, {
                apply(target, thisArg, args) {
                    record(typeof args[0] === "string"
                        ? args[0]
                        : (args[0] != null ? String(args[0]) : ""));
                    return Reflect.apply(target, thisArg, args);
                },
                construct(target, args, newTarget) {
                    record(typeof args[0] === "string"
                        ? args[0]
                        : (args[0] != null ? String(args[0]) : ""));
                    return Reflect.construct(target, args, newTarget);
                },
            });
        } catch (_) {}
    };
    // Fresh pages never go through __resetDomRegistries (pool-reuse only),
    // so the wrap must be installed here at bootstrap — the fn is
    // idempotent via __oxEvalTapReady.
    try { globalThis.__oxInstallEvalTap(); } catch (_) {}

    // Parent-side sink for `{__oxEvalSrc:{href,code}}` mirrors that doomed
    // child realms postMessage before a crashed_retry renavigation replaces
    // them (their realm-local __oxEvalSrcLog dies with the realm; this
    // survives in the top window for the end-of-run dump).
    try {
        globalThis.addEventListener("message", (ev) => {
            try {
                const d = ev && ev.data;
                if (!d || typeof d !== "object" || !d.__oxEvalSrc) return;
                const log = globalThis.__oxParentEvalSrc
                    || (globalThis.__oxParentEvalSrc = []);
                if (log.length >= 8) return;
                const code = d.__oxEvalSrc.code;
                for (let i = 0; i < log.length; i++) {
                    if (log[i] && log[i].code
                        && log[i].code.length === code.length) return;
                }
                log.push(d.__oxEvalSrc);
            } catch (_) {}
        });
    } catch (_) {}
})(globalThis);
