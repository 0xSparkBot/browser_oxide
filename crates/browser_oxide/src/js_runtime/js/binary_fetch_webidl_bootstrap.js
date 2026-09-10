// Deep WebIDL normalization for binary/file/fetch objects.  Functional
// implementations are installed earlier; this layer removes page-visible
// implementation fields while preserving the existing network/blob bridges.
((globalThis) => {
    const ops = Deno.core.ops;
    const _mask = (fn, name) => {
        if (typeof fn !== 'function') return fn;
        try {
            if (typeof globalThis._maskFunction === 'function') {
                globalThis._maskFunction(fn, name || fn.name);
            }
        } catch (_) {}
        return fn;
    };
    const _method = (proto, name, fn, length) => {
        try { Object.defineProperty(fn, 'name', { value: name, configurable: true }); } catch (_) {}
        if (length !== undefined) {
            try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        }
        _mask(fn, name);
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
        return fn;
    };
    const _getter = (proto, name, get, set) => {
        try { Object.defineProperty(get, 'name', { value: `get ${name}`, configurable: true }); } catch (_) {}
        try { Object.defineProperty(get, 'length', { value: 0, configurable: true }); } catch (_) {}
        _mask(get, `get ${name}`);
        if (set) {
            try { Object.defineProperty(set, 'name', { value: `set ${name}`, configurable: true }); } catch (_) {}
            try { Object.defineProperty(set, 'length', { value: 1, configurable: true }); } catch (_) {}
            _mask(set, `set ${name}`);
        }
        Object.defineProperty(proto, name, {
            get, set, enumerable: true, configurable: true,
        });
    };
    const _tag = (proto, name) => {
        Object.defineProperty(proto, Symbol.toStringTag, { value: name, configurable: true });
    };
    const _requireNew = (newTarget, name) => {
        if (!newTarget) {
            throw new TypeError(
                `Failed to construct '${name}': Please use the 'new' operator, this DOM object constructor cannot be called as a function.`
            );
        }
    };

    // -----------------------------------------------------------------
    // Blob / File
    // -----------------------------------------------------------------
    const blobState = new WeakMap();
    const _encoder = new TextEncoder();
    const _decoder = new TextDecoder();
    const _copyBytes = (bytes) => {
        const src = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || 0);
        const out = new Uint8Array(src.byteLength);
        out.set(src);
        return out;
    };
    const _blobBytes = (value) => {
        const state = blobState.get(value);
        if (state) return state.bytes;
        // Compatibility with a pre-normalization object created internally
        // during bootstrap. Page-created objects after this layer never expose
        // `_data` themselves.
        if (value && value._data instanceof Uint8Array) return value._data;
        return null;
    };
    const _normalizeBlobType = (value) => {
        const s = String(value ?? '');
        for (let i = 0; i < s.length; i++) {
            const code = s.charCodeAt(i);
            if (code < 0x20 || code > 0x7e) return '';
        }
        return s.toLowerCase();
    };
    function Blob(blobParts = [], options = {}) {
        _requireNew(new.target, 'Blob');
        const chunks = [];
        let total = 0;
        const list = blobParts == null ? [] : Array.from(blobParts);
        for (const part of list) {
            let bytes;
            const nested = _blobBytes(part);
            if (nested) bytes = nested;
            else if (typeof part === 'string') bytes = _encoder.encode(part);
            else if (part instanceof ArrayBuffer) bytes = new Uint8Array(part);
            else if (ArrayBuffer.isView(part)) bytes = new Uint8Array(part.buffer, part.byteOffset, part.byteLength);
            else bytes = _encoder.encode(String(part));
            chunks.push(bytes);
            total += bytes.byteLength;
        }
        const merged = new Uint8Array(total);
        let offset = 0;
        for (const bytes of chunks) { merged.set(bytes, offset); offset += bytes.byteLength; }
        blobState.set(this, { bytes: merged, type: _normalizeBlobType(options?.type) });
    }
    _mask(Blob, 'Blob');
    _getter(Blob.prototype, 'size', function() {
        const s = blobState.get(this); if (!s) throw new TypeError('Illegal invocation'); return s.bytes.byteLength;
    });
    _getter(Blob.prototype, 'type', function() {
        const s = blobState.get(this); if (!s) throw new TypeError('Illegal invocation'); return s.type;
    });
    _method(Blob.prototype, 'slice', function(start = 0, end = undefined, contentType = '') {
        const s = blobState.get(this); if (!s) throw new TypeError('Illegal invocation');
        const size = s.bytes.byteLength;
        let relativeStart = Number(start);
        if (!Number.isFinite(relativeStart)) relativeStart = relativeStart < 0 ? 0 : size;
        relativeStart = Math.trunc(relativeStart);
        relativeStart = relativeStart < 0 ? Math.max(size + relativeStart, 0) : Math.min(relativeStart, size);
        let relativeEnd = end === undefined ? size : Number(end);
        if (!Number.isFinite(relativeEnd)) relativeEnd = relativeEnd < 0 ? 0 : size;
        relativeEnd = Math.trunc(relativeEnd);
        relativeEnd = relativeEnd < 0 ? Math.max(size + relativeEnd, 0) : Math.min(relativeEnd, size);
        const span = Math.max(relativeEnd - relativeStart, 0);
        const out = new Blob([], { type: contentType });
        const outState = blobState.get(out);
        outState.bytes = s.bytes.slice(relativeStart, relativeStart + span);
        return out;
    }, 0);
    _method(Blob.prototype, 'text', function() {
        const s = blobState.get(this); if (!s) return Promise.reject(new TypeError('Illegal invocation'));
        return Promise.resolve(_decoder.decode(s.bytes));
    }, 0);
    _method(Blob.prototype, 'arrayBuffer', function() {
        const s = blobState.get(this); if (!s) return Promise.reject(new TypeError('Illegal invocation'));
        const copy = _copyBytes(s.bytes);
        return Promise.resolve(copy.buffer);
    }, 0);
    _method(Blob.prototype, 'bytes', function() {
        const s = blobState.get(this); if (!s) return Promise.reject(new TypeError('Illegal invocation'));
        return Promise.resolve(_copyBytes(s.bytes));
    }, 0);
    _method(Blob.prototype, 'stream', function() {
        const s = blobState.get(this); if (!s) throw new TypeError('Illegal invocation');
        const bytes = _copyBytes(s.bytes);
        return new ReadableStream({
            start(controller) {
                if (bytes.byteLength) controller.enqueue(bytes);
                controller.close();
            },
        });
    }, 0);
    _tag(Blob.prototype, 'Blob');
    globalThis.Blob = Blob;

    const fileState = new WeakMap();
    function File(fileBits, fileName, options = {}) {
        _requireNew(new.target, 'File');
        if (arguments.length < 2) {
            throw new TypeError(`Failed to construct 'File': 2 arguments required, but only ${arguments.length} present.`);
        }
        // Initialize the Blob base state through a temporary normalized Blob.
        const tmp = new Blob(fileBits, options);
        blobState.set(this, { bytes: _copyBytes(blobState.get(tmp).bytes), type: blobState.get(tmp).type });
        fileState.set(this, {
            name: String(fileName).replaceAll('/', ':'),
            lastModified: options?.lastModified === undefined ? Date.now() : Number(options.lastModified),
            webkitRelativePath: '',
        });
    }
    File.prototype = Object.create(Blob.prototype, {
        constructor: { value: File, writable: true, configurable: true },
    });
    Object.setPrototypeOf(File, Blob);
    _mask(File, 'File');
    _getter(File.prototype, 'name', function() { const s=fileState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.name; });
    _getter(File.prototype, 'lastModified', function() { const s=fileState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.lastModified; });
    _getter(File.prototype, 'lastModifiedDate', function() { const s=fileState.get(this); if(!s) throw new TypeError('Illegal invocation'); return new Date(s.lastModified); });
    _getter(File.prototype, 'webkitRelativePath', function() { const s=fileState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.webkitRelativePath; });
    _tag(File.prototype, 'File');
    globalThis.File = File;

    // Structured clone / Worker wire serialization needs synchronous access
    // to the normalized Blob backing bytes. Keep that access on the private
    // engine bridge rather than re-introducing a page-visible `_data` field.
    const blobSnapshotForClone = (value) => {
        const blob = blobState.get(value);
        if (!blob) return null;
        const file = fileState.get(value);
        return {
            bytes: blob.bytes,
            type: blob.type,
            isFile: !!file,
            name: file ? file.name : '',
            lastModified: file ? file.lastModified : 0,
        };
    };
    // Worker runtimes execute this normalization layer before
    // structured_clone.js creates its legacy private bridge. Ensure the bridge
    // already exists here so the Blob snapshot hook is captured in both Window
    // and DedicatedWorker realms; cleanup_bootstrap removes the global name
    // before page-authored code can observe it.
    if (!globalThis.__browser_oxide) globalThis.__browser_oxide = {};
    for (const bridge of [globalThis._browser_oxide, globalThis.__browser_oxide]) {
        if (!bridge) continue;
        try {
            Object.defineProperty(bridge, 'blobSnapshotForClone', {
                value: blobSnapshotForClone,
                configurable: true,
                enumerable: false,
                writable: false,
            });
        } catch (_) {}
    }

    // Keep the native blob URL registry path, but read normalized Blob bytes
    // from the WeakMap instead of a page-visible `_data` property.
    if (globalThis.URL && typeof globalThis.URL.createObjectURL === 'function') {
        const _oldRevoke = globalThis.URL.revokeObjectURL;
        const createObjectURL = function createObjectURL(obj) {
            const bytes = _blobBytes(obj);
            if (!bytes) throw new TypeError("Failed to execute 'createObjectURL' on 'URL': Overload resolution failed.");
            const u = 'blob:' + (globalThis.location?.origin || 'null') + '/' +
                (globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random()}`);
            ops.op_blob_register(u, bytes, String(obj.type || ''));
            return u;
        };
        try { Object.defineProperty(createObjectURL, 'length', { value: 1, configurable: true }); } catch (_) {}
        _mask(createObjectURL, 'createObjectURL');
        Object.defineProperty(globalThis.URL, 'createObjectURL', { value: createObjectURL, writable: true, configurable: true });
        if (typeof _oldRevoke === 'function') {
            try { Object.defineProperty(_oldRevoke, 'length', { value: 1, configurable: true }); } catch (_) {}
        }
    }

    // -----------------------------------------------------------------
    // FileReader
    // -----------------------------------------------------------------
    const readerState = new WeakMap();
    const readerHandlers = ['abort','error','load','loadend','loadstart','progress'];
    const _reader = (self) => { const s=readerState.get(self); if(!s) throw new TypeError('Illegal invocation'); return s; };
    const _readerEmit = (self, type) => {
        setTimeout(() => {
            let ev;
            try { ev = new ProgressEvent(type); } catch (_) { ev = new Event(type); }
            try { self.dispatchEvent(ev); } catch (_) {}
            const fn = _reader(self).handlers[type];
            if (typeof fn === 'function') { try { fn.call(self, ev); } catch (_) {} }
        }, 0);
    };
    function FileReader() {
        _requireNew(new.target, 'FileReader');
        const obj = Reflect.construct(EventTarget, [], new.target);
        readerState.set(obj, { readyState: 0, result: null, error: null, handlers: Object.create(null) });
        return obj;
    }
    FileReader.prototype = Object.create(EventTarget.prototype, {
        constructor: { value: FileReader, writable: true, configurable: true },
    });
    Object.setPrototypeOf(FileReader, EventTarget);
    _mask(FileReader, 'FileReader');
    for (const [name,value] of [['EMPTY',0],['LOADING',1],['DONE',2]]) {
        Object.defineProperty(FileReader, name, { value, enumerable: true });
        Object.defineProperty(FileReader.prototype, name, { value, enumerable: true });
    }
    _getter(FileReader.prototype, 'readyState', function(){ return _reader(this).readyState; });
    _getter(FileReader.prototype, 'result', function(){ return _reader(this).result; });
    _getter(FileReader.prototype, 'error', function(){ return _reader(this).error; });
    for (const type of readerHandlers) {
        _getter(FileReader.prototype, `on${type}`,
            function(){ return _reader(this).handlers[type] ?? null; },
            function(value){ _reader(this).handlers[type] = typeof value === 'function' ? value : null; });
    }
    const _readerStart = (self, blob, convert) => {
        const s = _reader(self);
        const bytes = _blobBytes(blob);
        if (!bytes) throw new TypeError("Failed to execute read on 'FileReader': parameter 1 is not of type 'Blob'.");
        s.readyState = 1; s.result = null; s.error = null; _readerEmit(self, 'loadstart');
        try {
            s.result = convert(bytes, blob);
            s.readyState = 2; _readerEmit(self, 'load'); _readerEmit(self, 'loadend');
        } catch (e) {
            s.error = e; s.result = null; s.readyState = 2; _readerEmit(self, 'error'); _readerEmit(self, 'loadend');
        }
    };
    _method(FileReader.prototype, 'readAsText', function(blob, encoding = 'utf-8') {
        _readerStart(this, blob, bytes => new TextDecoder(encoding || 'utf-8').decode(bytes));
    }, 1);
    _method(FileReader.prototype, 'readAsArrayBuffer', function(blob) {
        _readerStart(this, blob, bytes => _copyBytes(bytes).buffer);
    }, 1);
    _method(FileReader.prototype, 'readAsBinaryString', function(blob) {
        _readerStart(this, blob, bytes => {
            let out=''; const chunk=0x8000;
            for(let i=0;i<bytes.length;i+=chunk) out += String.fromCharCode.apply(null, bytes.subarray(i,i+chunk));
            return out;
        });
    }, 1);
    _method(FileReader.prototype, 'readAsDataURL', function(blob) {
        _readerStart(this, blob, bytes => {
            let bin=''; const chunk=0x8000;
            for(let i=0;i<bytes.length;i+=chunk) bin += String.fromCharCode.apply(null, bytes.subarray(i,i+chunk));
            return `data:${blob.type || 'application/octet-stream'};base64,${btoa(bin)}`;
        });
    }, 1);
    _method(FileReader.prototype, 'abort', function() {
        const s=_reader(this); s.readyState=2; s.result=null; s.error=null; _readerEmit(this,'abort'); _readerEmit(this,'loadend');
    }, 0);
    _tag(FileReader.prototype, 'FileReader');
    globalThis.FileReader = FileReader;

    // -----------------------------------------------------------------
    // Headers — preserve the existing private storage, normalize WebIDL
    // descriptors and add the two missing Chrome methods.
    // -----------------------------------------------------------------
    const HeadersCtor = globalThis.Headers;
    if (typeof HeadersCtor === 'function') {
        const hp = HeadersCtor.prototype;
        const oldSet = hp.set;
        const oldDelete = hp.delete;
        const setCookieLists = new WeakMap();
        _method(hp, 'append', function(name, value) {
            const key = String(name).toLowerCase();
            const text = String(value);
            if (key === 'set-cookie') {
                const list = setCookieLists.get(this) || [];
                list.push(text); setCookieLists.set(this, list);
            }
            const current = this.get(key);
            oldSet.call(this, key, current == null ? text : `${current}, ${text}`);
        }, 2);
        _method(hp, 'getSetCookie', function() {
            const explicit = setCookieLists.get(this);
            if (explicit) return explicit.slice();
            const value = this.get('set-cookie');
            return value == null ? [] : [value];
        }, 0);
        _method(hp, 'set', function(name, value) {
            const key=String(name).toLowerCase();
            if(key==='set-cookie') setCookieLists.set(this,[String(value)]);
            return oldSet.call(this,name,value);
        }, 2);
        _method(hp, 'delete', function(name) {
            if(String(name).toLowerCase()==='set-cookie') setCookieLists.delete(this);
            return oldDelete.call(this,name);
        }, 1);
        for (const [name,len] of [['get',1],['has',1],['forEach',1],['entries',0],['keys',0],['values',0]]) {
            const fn=hp[name]; if(typeof fn==='function') _method(hp,name,fn,len);
        }
        if (typeof hp.entries === 'function') {
            Object.defineProperty(hp, Symbol.iterator, { value: hp.entries, writable: true, configurable: true });
        }
        _tag(hp, 'Headers');
    }

    // -----------------------------------------------------------------
    // Request — wrap the functional constructor but return an instance of
    // its original prototype, so fetch_bootstrap's closed-over instanceof
    // checks continue to succeed. All former own fields move to WeakMap.
    // -----------------------------------------------------------------
    const OldRequest = globalThis.Request;
    const requestState = new WeakMap();
    const _bodyBytes = (body) => {
        if (body == null) return null;
        const bb=_blobBytes(body); if(bb) return _copyBytes(bb);
        if(typeof body==='string') return _encoder.encode(body);
        if(body instanceof ArrayBuffer) return _copyBytes(new Uint8Array(body));
        if(ArrayBuffer.isView(body)) return _copyBytes(new Uint8Array(body.buffer,body.byteOffset,body.byteLength));
        if(typeof URLSearchParams!=='undefined' && body instanceof URLSearchParams) return _encoder.encode(body.toString());
        return _encoder.encode(String(body));
    };
    const _requestBodyStream = (s) => {
        if (s.rawBody == null) return null;
        if (s.bodyStream) return s.bodyStream;
        const bytes=_bodyBytes(s.rawBody) || new Uint8Array();
        s.bodyStream = new ReadableStream({ start(c){ if(bytes.byteLength)c.enqueue(bytes); c.close(); } });
        return s.bodyStream;
    };
    function Request(input, init = {}) {
        _requireNew(new.target, 'Request');
        if (arguments.length < 1) throw new TypeError("Failed to construct 'Request': 1 argument required, but only 0 present.");
        init = init || {};
        const inherited = requestState.get(input);
        const oldInstance = !inherited && input instanceof OldRequest ? input : null;
        const inheritedValue = (name, fallback) => {
            if (inherited) return inherited[name];
            if (oldInstance) {
                try { return oldInstance[name]; } catch (_) {}
            }
            return fallback;
        };
        let url;
        if (inherited || oldInstance) url = String(inheritedValue('url', ''));
        else url = String(input);
        const rawBody = Object.prototype.hasOwnProperty.call(init, 'body')
            ? init.body
            : inheritedValue('rawBody', oldInstance ? inheritedValue('body', null) : null);
        const headersSource = Object.prototype.hasOwnProperty.call(init, 'headers')
            ? init.headers
            : inheritedValue('headers', {});
        const headers = new HeadersCtor(headersSource || {});
        const method = String(
            Object.prototype.hasOwnProperty.call(init, 'method')
                ? init.method
                : inheritedValue('method', 'GET')
        ).toUpperCase();
        let signal = Object.prototype.hasOwnProperty.call(init, 'signal')
            ? init.signal
            : inheritedValue('signal', null);
        if (signal == null && typeof globalThis.AbortController === 'function') {
            try { signal = new globalThis.AbortController().signal; } catch (_) {}
        }
        const obj = Object.create(new.target.prototype);
        const state={
            url, method, headers, rawBody, signal, bodyStream:null, used:false,
            cache:String(init.cache ?? inheritedValue('cache', 'default')),
            credentials:String(init.credentials ?? inheritedValue('credentials', 'same-origin')),
            destination:String(inheritedValue('destination', '')),
            integrity:String(init.integrity ?? inheritedValue('integrity', '')),
            keepalive:Boolean(init.keepalive ?? inheritedValue('keepalive', false)),
            mode:String(init.mode ?? inheritedValue('mode', 'cors')),
            redirect:String(init.redirect ?? inheritedValue('redirect', 'follow')),
            referrer:String(init.referrer ?? inheritedValue('referrer', 'about:client')),
            referrerPolicy:String(init.referrerPolicy ?? inheritedValue('referrerPolicy', '')),
            duplex:String(init.duplex ?? inheritedValue('duplex', 'half')),
            isHistoryNavigation:false,
            targetAddressSpace:String(init.targetAddressSpace ?? inheritedValue('targetAddressSpace', '')),
        };
        requestState.set(obj,state);
        return obj;
    }
    Request.prototype = OldRequest.prototype;
    Object.setPrototypeOf(Request, Object.getPrototypeOf(OldRequest));
    Object.defineProperty(Request.prototype,'constructor',{value:Request,writable:true,configurable:true});
    _mask(Request,'Request');
    const _rq = self => { const s=requestState.get(self); if(!s) throw new TypeError('Illegal invocation'); return s; };
    for(const name of ['url','method','headers','cache','credentials','destination','integrity','keepalive','mode','redirect','referrer','referrerPolicy','duplex','isHistoryNavigation','targetAddressSpace','signal']) {
        _getter(Request.prototype,name,function(){ return _rq(this)[name]; });
    }
    _getter(Request.prototype,'body',function(){ return _requestBodyStream(_rq(this)); });
    _getter(Request.prototype,'bodyUsed',function(){ const s=_rq(this); return s.used || !!s.bodyStream?.locked; });
    const _consumeRequest = async (self, kind) => {
        const s=_rq(self); if(s.used) throw new TypeError('Body has already been read'); s.used=true;
        const bytes=_bodyBytes(s.rawBody) || new Uint8Array();
        if(kind==='bytes') return _copyBytes(bytes);
        if(kind==='arrayBuffer') return _copyBytes(bytes).buffer;
        if(kind==='blob') return new Blob([bytes]);
        const text=_decoder.decode(bytes);
        if(kind==='json') return JSON.parse(text);
        if(kind==='formData') {
            const fd=new FormData();
            for(const [k,v] of new URLSearchParams(text)) fd.append(k,v);
            return fd;
        }
        return text;
    };
    for(const name of ['text','json','arrayBuffer','blob','bytes','formData']) {
        _method(Request.prototype,name,function(){ return _consumeRequest(this,name); },0);
    }
    _method(Request.prototype,'clone',function(){
        const s=_rq(this); if(s.used) throw new TypeError('Body has already been used');
        return new Request(s.url,{method:s.method,headers:s.headers,body:s.rawBody,signal:s.signal,cache:s.cache,credentials:s.credentials,integrity:s.integrity,keepalive:s.keepalive,mode:s.mode,redirect:s.redirect,referrer:s.referrer,referrerPolicy:s.referrerPolicy,duplex:s.duplex,targetAddressSpace:s.targetAddressSpace});
    },0);
    _tag(Request.prototype,'Request');
    globalThis.Request=Request;

    // Preserve fetch(Request) semantics even though Request.body now exposes a
    // standards-shaped ReadableStream rather than the raw body used internally.
    if (typeof globalThis.fetch === 'function') {
        const oldFetch=globalThis.fetch;
        const fetch = function fetch(input, init = {}) {
            const s=requestState.get(input);
            if(!s) return oldFetch(input,init);
            return oldFetch(s.url,{
                method:s.method, headers:s.headers, body:s.rawBody, signal:s.signal,
                mode:s.mode, credentials:s.credentials, ...init,
            });
        };
        try { Object.defineProperty(fetch,'length',{value:1,configurable:true}); } catch(_){}
        _mask(fetch,'fetch');
        globalThis.fetch=fetch;
    }

    // -----------------------------------------------------------------
    // Response — original private fields are already non-observable. Add the
    // missing standard members and normalize all WebIDL descriptors in-place.
    // -----------------------------------------------------------------
    const ResponseCtor=globalThis.Response;
    if(typeof ResponseCtor==='function') {
        const rp=ResponseCtor.prototype;
        const originalArrayBuffer=rp.arrayBuffer;
        _getter(rp,'type',function(){ return 'default'; });
        _getter(rp,'redirected',function(){ return false; });
        _method(rp,'bytes',async function(){ return new Uint8Array(await originalArrayBuffer.call(this)); },0);
        _method(rp,'blob',async function(){
            const bytes=new Uint8Array(await originalArrayBuffer.call(this));
            return new Blob([bytes],{type:this.headers?.get?.('content-type') || ''});
        },0);
        _method(rp,'formData',async function(){
            const text=await this.text(); const fd=new FormData();
            for(const [k,v] of new URLSearchParams(text)) fd.append(k,v);
            return fd;
        },0);
        for(const name of ['status','statusText','ok','headers','url','body','bodyUsed']) {
            const d=Object.getOwnPropertyDescriptor(rp,name); if(d?.get) _getter(rp,name,d.get,d.set);
        }
        for(const [name,len] of [['text',0],['json',0],['arrayBuffer',0],['clone',0]]) {
            const fn=rp[name]; if(typeof fn==='function') _method(rp,name,fn,len);
        }
        _tag(rp,'Response');
    }
})(globalThis);
