// WebIDL normalization for Element / HTMLElement and high-signal HTML element
// subclasses. The DOM backend remains unchanged; this layer relocates members
// to their Chrome prototype owners and fills standards-shaped reflection/state.
((globalThis) => {
    const domOps = Deno.core.ops;
    const getNodeIdForInnerText = globalThis.__browser_oxide?._getNodeId;
    // Internal bridges are removed from the public global by cleanup. Capture
    // the state-preserving DOM move helper now; never resolve it per call.
    const preserveDomMoveBefore = globalThis.__browser_oxide?.domMoveBeforePreservingState;
    const mask = (fn, name) => {
        if (typeof fn !== 'function') return fn;
        try {
            Object.defineProperty(fn, 'name', { value: name || fn.name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name || fn.name);
        } catch (_) {}
        return fn;
    };
    const setLength = (fn, length) => {
        if (typeof fn !== 'function') return;
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
    };
    const method = (proto, name, fn, length) => {
        setLength(fn, length); mask(fn, name);
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };
    const getter = (proto, name, get, set) => {
        setLength(get, 0); mask(get, `get ${name}`);
        if (set) { setLength(set, 1); mask(set, `set ${name}`); }
        Object.defineProperty(proto, name, {
            get, set, enumerable: true, configurable: true,
        });
    };
    const makeEnumerable = (proto) => {
        for (const name of Object.getOwnPropertyNames(proto)) {
            if (name === 'constructor') continue;
            const d = Object.getOwnPropertyDescriptor(proto, name);
            if (!d || !d.configurable) continue;
            d.enumerable = true;
            Object.defineProperty(proto, name, d);
        }
    };
    const reflectString = (proto, prop, attr = prop.toLowerCase(), defaultValue = '') => {
        getter(proto, prop,
            function() { const v = this.getAttribute(attr); return v == null ? defaultValue : v; },
            function(value) { this.setAttribute(attr, String(value)); });
    };
    const reflectNullable = (proto, prop, attr) => {
        getter(proto, prop,
            function() { const v = this.getAttribute(attr); return v == null ? null : v; },
            function(value) { if (value == null) this.removeAttribute(attr); else this.setAttribute(attr, String(value)); });
    };
    const reflectBoolean = (proto, prop, attr = prop.toLowerCase()) => {
        getter(proto, prop,
            function() { return this.hasAttribute(attr); },
            function(value) { if (value) this.setAttribute(attr, ''); else this.removeAttribute(attr); });
    };
    const reflectNumber = (proto, prop, attr = prop.toLowerCase(), defaultValue = 0) => {
        getter(proto, prop,
            function() {
                const raw = this.getAttribute(attr);
                if (raw == null || raw === '') return defaultValue;
                const n = Number(raw); return Number.isFinite(n) ? Math.trunc(n) : defaultValue;
            },
            function(value) { this.setAttribute(attr, String(Math.trunc(Number(value) || 0))); });
    };
    const copyDescriptor = (from, to, name) => {
        if (!from || !to) return null;
        const d = Object.getOwnPropertyDescriptor(from, name);
        if (!d) return null;
        const copy = { ...d, enumerable: true };
        try { Object.defineProperty(to, name, copy); } catch (_) {}
        return d;
    };
    const illegalWrapper = (name, Original, parentCtor) => {
        if (typeof Original !== 'function' || !Original.prototype) return Original;
        const Ctor = function() { throw new TypeError('Illegal constructor'); };
        try { Object.defineProperty(Ctor, 'name', { value: name, configurable: true }); } catch (_) {}
        setLength(Ctor, 0);
        try { Ctor.prototype = Original.prototype; } catch (_) {}
        try { Object.setPrototypeOf(Ctor, parentCtor || Object.getPrototypeOf(Original)); } catch (_) {}
        try {
            Object.defineProperty(Original.prototype, 'constructor', {
                value: Ctor, writable: true, enumerable: false, configurable: true,
            });
        } catch (_) {}
        mask(Ctor, name);
        globalThis[name] = Ctor;
        return Ctor;
    };

    const ElementOriginal = globalThis.Element;
    const HTMLElementOriginal = globalThis.HTMLElement;
    const DivOriginal = globalThis.HTMLDivElement;
    const InputOriginal = globalThis.HTMLInputElement;
    const TextAreaOriginal = globalThis.HTMLTextAreaElement;
    const IFrameOriginal = globalThis.HTMLIFrameElement;
    const ep = ElementOriginal?.prototype;
    const hp = HTMLElementOriginal?.prototype;
    const dp = DivOriginal?.prototype;
    const ip = InputOriginal?.prototype;
    const tp = TextAreaOriginal?.prototype;
    const fp = IFrameOriginal?.prototype;
    if (!ep || !hp) return;

    const syncSelectorFormValue = (self, value) => {
        try {
            if (typeof getNodeIdForInnerText !== 'function'
                || typeof domOps.op_dom_set_form_control_value !== 'function') return;
            const id = getNodeIdForInnerText(self);
            if (Number.isInteger(id) && id >= 0) {
                domOps.op_dom_set_form_control_value(id, String(value));
            }
        } catch (_) {}
    };

    // Capture the historically over-broad Element members before relocating.
    const elementDescriptors = Object.create(null);
    for (const name of [
        'src','href','type','rel','async','defer','crossOrigin','integrity','referrerPolicy',
        'style','dataset','offsetHeight','offsetLeft','offsetParent','offsetTop','offsetWidth',
        'focus','blur','click',
    ]) elementDescriptors[name] = Object.getOwnPropertyDescriptor(ep, name) || null;

    // Keep specific HTML element behavior when the generic attributes leave
    // Element.prototype.
    const specificTargets = {
        src: ['HTMLScriptElement','HTMLImageElement','HTMLIFrameElement','HTMLVideoElement','HTMLAudioElement'],
        href: ['HTMLAnchorElement','HTMLLinkElement'],
        type: ['HTMLScriptElement','HTMLLinkElement','HTMLStyleElement'],
        rel: ['HTMLAnchorElement','HTMLLinkElement'],
        async: ['HTMLScriptElement'],
        defer: ['HTMLScriptElement'],
        crossOrigin: ['HTMLScriptElement','HTMLImageElement','HTMLLinkElement','HTMLVideoElement','HTMLAudioElement'],
        integrity: ['HTMLScriptElement','HTMLLinkElement'],
        referrerPolicy: ['HTMLScriptElement','HTMLImageElement','HTMLIFrameElement','HTMLLinkElement','HTMLAnchorElement'],
    };
    for (const [name, targets] of Object.entries(specificTargets)) {
        const d = elementDescriptors[name];
        if (!d) continue;
        for (const targetName of targets) {
            const proto = globalThis[targetName]?.prototype;
            if (proto && !Object.prototype.hasOwnProperty.call(proto, name)) {
                try { Object.defineProperty(proto, name, { ...d, enumerable: true }); } catch (_) {}
            }
        }
    }

    // HTML hypertext/link elements reflect `hreflang` as a DOMString.
    // This is deliberately installed on the concrete interfaces rather than
    // HTMLElement/Element: Chrome exposes an own WebIDL accessor on each of
    // HTMLAnchorElement, HTMLAreaElement and HTMLLinkElement. Without it,
    // assigning `el.hreflang` merely creates an unrelated instance property
    // while the `hreflang` content attribute stays unchanged.
    for (const targetName of ['HTMLAnchorElement', 'HTMLAreaElement', 'HTMLLinkElement']) {
        const proto = globalThis[targetName]?.prototype;
        if (proto && !Object.prototype.hasOwnProperty.call(proto, 'hreflang')) {
            reflectString(proto, 'hreflang', 'hreflang');
        }
    }

    // Iframe set/removeAttribute carry the navigation hooks but belong on
    // Element.prototype in Chrome. Lift their behavior into an iframe-aware
    // base wrapper, then remove the iframe-own methods.
    const baseSetAttribute = ep.setAttribute;
    const baseRemoveAttribute = ep.removeAttribute;
    const iframeSetAttribute = fp && Object.getOwnPropertyDescriptor(fp, 'setAttribute')?.value;
    const iframeRemoveAttribute = fp && Object.getOwnPropertyDescriptor(fp, 'removeAttribute')?.value;
    if (typeof baseSetAttribute === 'function') {
        method(ep, 'setAttribute', function setAttribute(name, value) {
            if (fp && fp.isPrototypeOf(this) && typeof iframeSetAttribute === 'function') {
                return iframeSetAttribute.call(this, name, value);
            }
            return baseSetAttribute.call(this, name, value);
        }, 2);
    }
    if (typeof baseRemoveAttribute === 'function') {
        method(ep, 'removeAttribute', function removeAttribute(name) {
            if (fp && fp.isPrototypeOf(this) && typeof iframeRemoveAttribute === 'function') {
                return iframeRemoveAttribute.call(this, name);
            }
            return baseRemoveAttribute.call(this, name);
        }, 1);
    }
    if (fp) {
        try { delete fp.setAttribute; } catch (_) {}
        try { delete fp.removeAttribute; } catch (_) {}
    }

    // ---------------------------------------------------------------
    // Element
    // ---------------------------------------------------------------
    const classListDescriptor = Object.getOwnPropertyDescriptor(ep, 'classList');
    if (classListDescriptor?.get) {
        getter(ep, 'classList', classListDescriptor.get, function classList(value) {
            this.className = String(value);
        });
    }
    const outerHTMLDescriptor = Object.getOwnPropertyDescriptor(ep, 'outerHTML');
    if (outerHTMLDescriptor?.get && !outerHTMLDescriptor.set) {
        getter(ep, 'outerHTML', outerHTMLDescriptor.get, function outerHTML(value) {
            if (!this.parentNode) return;
            this.insertAdjacentHTML('beforebegin', String(value));
            this.remove();
        });
    }

    const ariaStrings = [
        'ariaAtomic','ariaAutoComplete','ariaBrailleLabel','ariaBrailleRoleDescription','ariaBusy',
        'ariaChecked','ariaColCount','ariaColIndex','ariaColIndexText','ariaColSpan','ariaCurrent',
        'ariaDescription','ariaDisabled','ariaExpanded','ariaHasPopup','ariaHidden','ariaInvalid',
        'ariaKeyShortcuts','ariaLabel','ariaLevel','ariaLive','ariaModal','ariaMultiLine',
        'ariaMultiSelectable','ariaOrientation','ariaPlaceholder','ariaPosInSet','ariaPressed',
        'ariaReadOnly','ariaRelevant','ariaRequired','ariaRoleDescription','ariaRowCount','ariaRowIndex',
        'ariaRowIndexText','ariaRowSpan','ariaSelected','ariaSetSize','ariaSort','ariaValueMax',
        'ariaValueMin','ariaValueNow','ariaValueText',
    ];
    const ariaAttr = name => 'aria-' + name.slice(4).replace(/([A-Z])/g, '-$1').replace(/^-/, '').toLowerCase();
    for (const name of ariaStrings) reflectNullable(ep, name, ariaAttr(name));
    reflectNullable(ep, 'role', 'role');

    const ariaRelations = new WeakMap();
    const relationDefaults = {
        ariaActiveDescendantElement: null,
        ariaControlsElements: [],
        ariaDescribedByElements: [],
        ariaDetailsElements: [],
        ariaErrorMessageElements: [],
        ariaFlowToElements: [],
        ariaLabelledByElements: [],
    };
    for (const [name, fallback] of Object.entries(relationDefaults)) {
        getter(ep, name,
            function() {
                const state = ariaRelations.get(this);
                if (state && Object.prototype.hasOwnProperty.call(state, name)) return state[name];
                return Array.isArray(fallback) ? [] : fallback;
            },
            function(value) {
                let state = ariaRelations.get(this);
                if (!state) { state = Object.create(null); ariaRelations.set(this, state); }
                state[name] = Array.isArray(fallback)
                    ? (value == null ? [] : Array.from(value))
                    : (value == null ? null : value);
            });
    }
    method(ep, 'ariaNotify', function ariaNotify() {}, 1);

    const elementHandlers = new WeakMap();
    const handlerNames = [
        'onbeforecopy','onbeforecut','onbeforepaste','onfullscreenchange','onfullscreenerror',
        'onsearch','onwebkitfullscreenchange','onwebkitfullscreenerror',
    ];
    for (const name of handlerNames) {
        const existing = Object.getOwnPropertyDescriptor(hp, name);
        if (existing) {
            try { Object.defineProperty(ep, name, { ...existing, enumerable: true }); } catch (_) {}
            try { delete hp[name]; } catch (_) {}
        } else {
            getter(ep, name,
                function() { return elementHandlers.get(this)?.[name] ?? null; },
                function(value) {
                    let state = elementHandlers.get(this);
                    if (!state) { state = Object.create(null); elementHandlers.set(this, state); }
                    state[name] = typeof value === 'function' ? value : null;
                });
        }
    }
    for (const wrong of [
        'onfreeze','onpointerlockchange','onpointerlockerror','onprerenderingchange',
        'onreadystatechange','onresume','onvisibilitychange',
    ]) {
        try { delete hp[wrong]; } catch (_) {}
    }

    const pointerCapture = new WeakMap();
    getter(ep, 'activeViewTransition', function activeViewTransition() { return null; });
    getter(ep, 'assignedSlot', function assignedSlot() { return null; });
    getter(ep, 'clientLeft', function clientLeft() { return 0; });
    getter(ep, 'clientTop', function clientTop() { return 0; });
    method(ep, 'computedStyleMap', function computedStyleMap() {
        const map = Object.create(globalThis.StylePropertyMapReadOnly?.prototype || Object.prototype);
        return map;
    }, 0);
    getter(ep, 'currentCSSZoom', function currentCSSZoom() { return 1; });
    getter(ep, 'customElementRegistry', function customElementRegistry() { return globalThis.customElements || null; });
    reflectString(ep, 'elementTiming', 'elementtiming', '');
    method(ep, 'getAttributeNames', function getAttributeNames() {
        const out = [];
        const attrs = this.attributes;
        for (let i = 0; attrs && i < attrs.length; i++) {
            const a = attrs.item(i); if (a) out.push(a.name);
        }
        return out;
    }, 0);
    method(ep, 'getElementsByTagNameNS', function getElementsByTagNameNS(namespace, localName) {
        const name = String(localName);
        const all = this.getElementsByTagName(name === '*' ? '*' : name);
        const wanted = namespace == null ? null : String(namespace);
        if (wanted === '*' || wanted == null || wanted === 'http://www.w3.org/1999/xhtml') return all;
        return all;
    }, 2);
    method(ep, 'getHTML', function getHTML() { return this.innerHTML; }, 0);
    method(ep, 'hasAttributes', function hasAttributes() { return !!(this.attributes && this.attributes.length); }, 0);
    method(ep, 'hasPointerCapture', function hasPointerCapture(pointerId) {
        return pointerCapture.get(this)?.has(Number(pointerId)) || false;
    }, 1);
    method(ep, 'setPointerCapture', function setPointerCapture(pointerId) {
        let set = pointerCapture.get(this); if (!set) { set = new Set(); pointerCapture.set(this, set); }
        set.add(Number(pointerId));
    }, 1);
    method(ep, 'releasePointerCapture', function releasePointerCapture(pointerId) {
        pointerCapture.get(this)?.delete(Number(pointerId));
    }, 1);
    method(ep, 'moveBefore', function moveBefore(movedNode, referenceNode) {
        if (typeof preserveDomMoveBefore === 'function') {
            return preserveDomMoveBefore(this, movedNode, referenceNode == null ? null : referenceNode);
        }
        return this.insertBefore(movedNode, referenceNode == null ? null : referenceNode);
    }, 2);
    method(ep, 'requestFullscreen', function requestFullscreen() { return Promise.resolve(); }, 0);
    method(ep, 'requestPointerLock', function requestPointerLock() { return Promise.resolve(); }, 0);
    method(ep, 'scroll', function scroll() { return this.scrollTo.apply(this, arguments); }, 0);
    method(ep, 'scrollIntoViewIfNeeded', function scrollIntoViewIfNeeded() {}, 0);
    method(ep, 'setHTML', function setHTML(html) { this.innerHTML = String(html); }, 1);
    method(ep, 'setHTMLUnsafe', function setHTMLUnsafe(html) { this.innerHTML = String(html); }, 1);
    method(ep, 'startViewTransition', function startViewTransition(callback) {
        if (globalThis.document?.startViewTransition) return globalThis.document.startViewTransition(callback);
        if (typeof callback === 'function') callback();
        return globalThis.ViewTransition ? Object.create(globalThis.ViewTransition.prototype) : {};
    }, 0);
    method(ep, 'webkitMatchesSelector', function webkitMatchesSelector(selector) { return this.matches(selector); }, 1);
    method(ep, 'webkitRequestFullScreen', function webkitRequestFullScreen() { return this.requestFullscreen(); }, 0);
    method(ep, 'webkitRequestFullscreen', function webkitRequestFullscreen() { return this.requestFullscreen(); }, 0);
    reflectString(ep, 'slot', 'slot', '');

    // Attribute-backed DOMTokenList used by Element.part and iframe.sandbox.
    const tokenCaches = new WeakMap();
    const tokenListFor = (element, attr) => {
        let byAttr = tokenCaches.get(element);
        if (!byAttr) { byAttr = new Map(); tokenCaches.set(element, byAttr); }
        if (byAttr.has(attr)) return byAttr.get(attr);
        const tokens = () => String(element.getAttribute(attr) || '').trim().split(/\s+/).filter(Boolean);
        const write = list => {
            const unique = Array.from(new Set(list.map(String).filter(Boolean)));
            if (unique.length) element.setAttribute(attr, unique.join(' ')); else element.removeAttribute(attr);
        };
        const target = Object.create(globalThis.DOMTokenList?.prototype || Object.prototype);
        const funcs = {
            item(index) { return tokens()[Number(index)] ?? null; },
            contains(token) { return tokens().includes(String(token)); },
            add() { const list=tokens(); for(const v of arguments) if(!list.includes(String(v))) list.push(String(v)); write(list); },
            remove() { const remove=new Set(Array.from(arguments,String)); write(tokens().filter(v=>!remove.has(v))); },
            toggle(token, force) {
                const value=String(token), list=tokens(), has=list.includes(value);
                if(force===true || (!has && force!==false)){ if(!has)list.push(value); write(list); return true; }
                if(has){ write(list.filter(v=>v!==value)); } return false;
            },
            replace(token, newToken) { const list=tokens(), i=list.indexOf(String(token)); if(i<0)return false; list[i]=String(newToken); write(list); return true; },
            supports() { return false; },
            entries() { return tokens().entries(); },
            keys() { return tokens().keys(); },
            values() { return tokens().values(); },
            forEach(cb, thisArg) { tokens().forEach((v,i)=>cb.call(thisArg,v,i,proxy)); },
            toString() { return tokens().join(' '); },
        };
        for (const [name, fn] of Object.entries(funcs)) mask(fn, name);
        const proxy = new Proxy(target, {
            get(_t, key) {
                if (key === 'length') return tokens().length;
                if (key === 'value') return tokens().join(' ');
                if (key === Symbol.iterator) return funcs.values;
                if (key === Symbol.toStringTag) return 'DOMTokenList';
                if (typeof key === 'string' && /^\d+$/.test(key)) return tokens()[Number(key)];
                if (Object.prototype.hasOwnProperty.call(funcs,key)) return funcs[key];
                return Reflect.get(target,key,proxy);
            },
            set(_t,key,value) {
                if(key==='value'){ write(String(value).trim().split(/\s+/)); return true; }
                return false;
            },
            has(_t,key) {
                if(typeof key==='string' && /^\d+$/.test(key)) return Number(key)<tokens().length;
                return key==='length'||key==='value'||key===Symbol.iterator||Object.prototype.hasOwnProperty.call(funcs,key);
            },
            ownKeys(){ return tokens().map((_,i)=>String(i)); },
            getOwnPropertyDescriptor(_t,key){
                if(typeof key==='string' && /^\d+$/.test(key) && Number(key)<tokens().length){
                    return {value:tokens()[Number(key)],writable:false,enumerable:true,configurable:true};
                }
            },
        });
        byAttr.set(attr, proxy); return proxy;
    };
    getter(ep, 'part', function part() { return tokenListFor(this, 'part'); }, function part(value) {
        this.setAttribute('part', String(value));
    });

    // Correct existing arities and WebIDL enumerability.
    for (const [name, length] of [['animate',1],['attachShadow',1],['scrollBy',0],['scrollIntoView',0],['scrollTo',0],['toggleAttribute',1]]) {
        const fn = ep[name]; if (typeof fn === 'function') setLength(fn, length);
    }

    // ---------------------------------------------------------------
    // HTMLElement — relocate HTMLElement-only APIs from Element.
    // ---------------------------------------------------------------
    const styleState = new WeakMap();
    const oldStyle = elementDescriptors.style;
    if (oldStyle?.get) {
        getter(hp, 'style', function style() {
            let result = styleState.get(this);
            if (!result) {
                result = oldStyle.get.call(this);
                try { delete this._style; } catch (_) {}
                styleState.set(this, result);
            }
            return result;
        }, function style(value) { this.style.cssText = String(value); });
        if (globalThis.SVGElement?.prototype) {
            getter(globalThis.SVGElement.prototype, 'style', function style() {
                let result=styleState.get(this); if(!result){ result=oldStyle.get.call(this); try{delete this._style;}catch(_){} styleState.set(this,result); } return result;
            }, function style(value){ this.style.cssText=String(value); });
        }
    }
    const datasetState = new WeakMap();
    const oldDataset = elementDescriptors.dataset;
    if (oldDataset?.get) {
        getter(hp, 'dataset', function dataset() {
            let result=datasetState.get(this); if(!result){ result=oldDataset.get.call(this); datasetState.set(this,result); } return result;
        });
        if (globalThis.SVGElement?.prototype) {
            getter(globalThis.SVGElement.prototype, 'dataset', function dataset() {
                let result=datasetState.get(this); if(!result){ result=oldDataset.get.call(this); datasetState.set(this,result); } return result;
            });
        }
    }
    for (const name of ['offsetHeight','offsetLeft','offsetParent','offsetTop','offsetWidth']) copyDescriptor(ep, hp, name);
    for (const name of ['focus','blur','click']) copyDescriptor(ep, hp, name);

    const typedStyleMaps = new WeakMap();
    const editContexts = new WeakMap();
    const internals = new WeakMap();
    const popoverOpen = new WeakSet();

    // `innerText` is rendered text, not a textContent alias.  Keep this
    // implementation in the WebIDL overlay instead of the DOM core so the
    // arena/tree backend stays independent from CSS presentation details.
    // The full CSSOM rendered-text algorithm is large; these helpers model the
    // browser-visible semantics that matter for ordinary document extraction:
    // hidden/non-rendered descendants, whitespace collapsing, BR/block line
    // boundaries, paragraphs, PRE whitespace, and table cell separators.
    const innerTextBlockTags = new Set([
        'ADDRESS','ARTICLE','ASIDE','BLOCKQUOTE','BODY','DD','DIV','DL','DT',
        'FIELDSET','FIGCAPTION','FIGURE','FOOTER','FORM','H1','H2','H3','H4',
        'H5','H6','HEADER','HGROUP','HR','LI','MAIN','NAV','OL','PRE','SECTION',
        'TABLE','TR','UL',
    ]);
    const innerTextExcludedTags = new Set(['SCRIPT','STYLE','NOSCRIPT','TEMPLATE','HEAD']);
    const innerTextParagraphTags = new Set(['P']);
    const innerTextPreWhiteSpace = new Set(['pre','pre-wrap','break-spaces']);
    const safeComputedStyle = (el) => {
        try { return typeof globalThis.getComputedStyle === 'function' ? globalThis.getComputedStyle(el) : null; }
        catch (_) { return null; }
    };
    const isInnerTextRendered = (el) => {
        try { if (!el.isConnected) return false; } catch (_) { return false; }
        for (let cur = el; cur && cur.nodeType === 1; cur = cur.parentElement) {
            const style = safeComputedStyle(cur);
            if (style && String(style.display || '').toLowerCase() === 'none') return false;
        }
        return true;
    };
    const renderedInnerText = (root) => {
        // HTML says a node that is not being rendered falls back to
        // textContent.  This is observable for detached/display:none roots.
        if (!isInnerTextRendered(root)) return root.textContent || '';

        let output = '';
        const ensureBreaks = (count) => {
            if (!output) return;
            output = output.replace(/[\t ]+$/g, '');
            let have = 0;
            for (let i = output.length - 1; i >= 0 && output[i] === '\n'; i--) have++;
            if (have < count) output += '\n'.repeat(count - have);
        };
        const appendText = (value, preserveWhiteSpace) => {
            let text = String(value == null ? '' : value).replace(/\r\n?/g, '\n');
            if (!preserveWhiteSpace) {
                text = text.replace(/[\t\n\f\r ]+/g, ' ');
                if (!text) return;
                if (!output || output.endsWith('\n')) text = text.replace(/^ +/, '');
                else if (output.endsWith(' ') && text.startsWith(' ')) text = text.slice(1);
            }
            output += text;
        };
        const walk = (node, inheritedWhiteSpace = 'normal', inheritedVisibility = 'visible') => {
            if (!node) return;
            if (node.nodeType === 3) {
                if (inheritedVisibility !== 'hidden' && inheritedVisibility !== 'collapse') {
                    appendText(node.data ?? node.textContent ?? '', innerTextPreWhiteSpace.has(inheritedWhiteSpace));
                }
                return;
            }
            if (node.nodeType !== 1) return;

            const tag = String(node.tagName || node.nodeName || '').toUpperCase();
            if (innerTextExcludedTags.has(tag)) return;
            const style = safeComputedStyle(node);
            const display = String(style?.display || '').toLowerCase();
            if (display === 'none') return;
            const visibility = String(style?.visibility || inheritedVisibility || 'visible').toLowerCase();
            // BrowserOxide's layout defaults intentionally stay lightweight,
            // so computed `display` is not yet a reliable source for the
            // inline-vs-block default of every HTML tag (eg. SPAN can report
            // block).  Use semantic HTML defaults for line boundaries and
            // reserve computed style for visibility/display:none.  Explicit
            // inline display styles can still override the semantic default.
            const explicitDisplay = String(node.style?.display || '').toLowerCase();
            const whiteSpace = tag === 'PRE'
                ? 'pre'
                : String(node.style?.whiteSpace || style?.whiteSpace || inheritedWhiteSpace || 'normal').toLowerCase();

            if (tag === 'BR') { if (visibility !== 'hidden' && visibility !== 'collapse') ensureBreaks(1); return; }

            const paragraph = innerTextParagraphTags.has(tag);
            const block = paragraph || innerTextBlockTags.has(tag) ||
                ['block','flow-root','list-item','table','table-row','flex','grid'].includes(explicitDisplay);
            if (block) ensureBreaks(paragraph ? 2 : 1);

            const children = node.childNodes ? Array.from(node.childNodes) : [];
            for (let i = 0; i < children.length; i++) {
                const child = children[i];
                // Chrome separates rendered table cells with a tab.
                if (i > 0 && (tag === 'TR') && child?.nodeType === 1) {
                    const ctag = String(child.tagName || child.nodeName || '').toUpperCase();
                    if ((ctag === 'TD' || ctag === 'TH') && !output.endsWith('\n') && !output.endsWith('\t')) output += '\t';
                }
                walk(child, whiteSpace, visibility);
            }
            if (block) ensureBreaks(paragraph ? 2 : 1);
        };

        // The root itself establishes visibility/white-space, but should not
        // introduce its own block boundary into its returned innerText.
        const rootStyle = safeComputedStyle(root);
        const rootVisibility = String(rootStyle?.visibility || 'visible').toLowerCase();
        const rootTag = String(root.tagName || root.nodeName || '').toUpperCase();
        const rootWhiteSpace = rootTag === 'PRE'
            ? 'pre'
            : String(root.style?.whiteSpace || rootStyle?.whiteSpace || 'normal').toLowerCase();
        for (const child of (root.childNodes ? Array.from(root.childNodes) : [])) {
            walk(child, rootWhiteSpace, rootVisibility);
        }
        return output.replace(/^\n+|\n+$/g, '');
    };
    const textReplacementNodes = (el, value) => {
        const doc = el.ownerDocument || globalThis.document;
        const lines = String(value).replace(/\r\n?/g, '\n').split('\n');
        const nodes = [];
        for (let i = 0; i < lines.length; i++) {
            if (i > 0) nodes.push(doc.createElement('br'));
            if (lines[i] !== '') nodes.push(doc.createTextNode(lines[i]));
        }
        return nodes;
    };
    reflectString(hp,'accessKey','accesskey','');
    reflectString(hp,'autocapitalize','autocapitalize','');
    reflectBoolean(hp,'autofocus','autofocus');
    reflectString(hp,'contentEditable','contenteditable','inherit');
    reflectString(hp,'dir','dir','');
    getter(hp,'draggable',function draggable(){ return this.getAttribute('draggable') === 'true'; },function draggable(v){ this.setAttribute('draggable',v?'true':'false'); });
    getter(hp,'editContext',function editContext(){ return editContexts.get(this)||null; },function editContext(v){ editContexts.set(this,v==null?null:v); });
    reflectString(hp,'enterKeyHint','enterkeyhint','');
    reflectBoolean(hp,'hidden','hidden');
    reflectBoolean(hp,'inert','inert');
    getter(hp,'innerText',function innerText(){
        if (typeof domOps.op_dom_get_inner_text === 'function' && typeof getNodeIdForInnerText === 'function') {
            return domOps.op_dom_get_inner_text(getNodeIdForInnerText(this));
        }
        return renderedInnerText(this);
    },function innerText(v){
        const nodes = textReplacementNodes(this, v);
        if (typeof this.replaceChildren === 'function') this.replaceChildren(...nodes);
        else {
            while (this.firstChild) this.removeChild(this.firstChild);
            for (const node of nodes) this.appendChild(node);
        }
    });
    reflectString(hp,'inputMode','inputmode','');
    getter(hp,'isContentEditable',function isContentEditable(){ const v=this.getAttribute('contenteditable'); return v===''||String(v).toLowerCase()==='true'; });
    reflectString(hp,'lang','lang','');
    reflectString(hp,'nonce','nonce','');
    getter(hp,'outerText',function outerText(){ return this.innerText; },function outerText(v){
        const parent = this.parentNode;
        if (!parent) throw new DOMException("Failed to set the 'outerText' property on 'HTMLElement': This element has no parent node.", 'NoModificationAllowedError');
        const nodes = textReplacementNodes(this, v);
        const ref = this;
        for (const node of nodes) parent.insertBefore(node, ref);
        parent.removeChild(this);
    });
    reflectNullable(hp,'popover','popover');
    getter(hp,'spellcheck',function spellcheck(){ const v=this.getAttribute('spellcheck'); return v==null ? true : String(v).toLowerCase()!=='false'; },function spellcheck(v){ this.setAttribute('spellcheck',v?'true':'false'); });
    reflectNumber(hp,'tabIndex','tabindex',-1);
    reflectString(hp,'title','title','');
    getter(hp,'translate',function translate(){ return String(this.getAttribute('translate')||'').toLowerCase()!=='no'; },function translate(v){ this.setAttribute('translate',v?'yes':'no'); });
    reflectString(hp,'virtualKeyboardPolicy','virtualkeyboardpolicy','');
    reflectString(hp,'writingSuggestions','writingsuggestions','');
    getter(hp,'attributeStyleMap',function attributeStyleMap(){
        let map=typedStyleMaps.get(this); if(!map){ map=Object.create(globalThis.StylePropertyMap?.prototype||Object.prototype); typedStyleMaps.set(this,map); } return map;
    });
    method(hp,'attachInternals',function attachInternals(){
        let value=internals.get(this); if(!value){ value=Object.create(globalThis.ElementInternals?.prototype||Object.prototype); internals.set(this,value); } return value;
    },0);
    method(hp,'showPopover',function showPopover(){ popoverOpen.add(this); },0);
    method(hp,'hidePopover',function hidePopover(){ popoverOpen.delete(this); },0);
    method(hp,'togglePopover',function togglePopover(force){
        const open=popoverOpen.has(this); const next=force===undefined?!open:!!force; if(next)popoverOpen.add(this);else popoverOpen.delete(this); return next;
    },0);

    // Remove APIs that do not belong to Element once their behavior is safely
    // installed on HTMLElement / concrete HTML element prototypes.
    for (const name of [
        'src','href','type','rel','async','defer','crossOrigin','integrity','referrerPolicy',
        'style','dataset','offsetHeight','offsetLeft','offsetParent','offsetTop','offsetWidth','focus','blur','click',
    ]) { try { delete ep[name]; } catch (_) {} }

    // ---------------------------------------------------------------
    // HTMLDivElement
    // ---------------------------------------------------------------
    if (dp) reflectString(dp,'align','align','');

    // ---------------------------------------------------------------
    // HTMLInputElement
    // ---------------------------------------------------------------
    if (ip) {
        const inputState = new WeakMap();
        const state = self => {
            let s=inputState.get(self);
            if(!s){
                const initialValue = self.getAttribute('value') || '';
                s={
                    files:null, indeterminate:false, customValidity:'',
                    selectionStart:0, selectionEnd:0, selectionDirection:'none',
                    value:initialValue, valueDirty:false,
                    checked:self.hasAttribute('checked'), checkedDirty:false,
                };
                inputState.set(self,s);
            }
            return s;
        };
        for (const [prop,attr,def] of [
            ['accept','accept',''],['align','align',''],['alt','alt',''],['autocomplete','autocomplete',''],
            ['dirName','dirname',''],['formAction','formaction',''],['formEnctype','formenctype',''],
            ['formMethod','formmethod',''],['formTarget','formtarget',''],['max','max',''],['min','min',''],
            ['pattern','pattern',''],['src','src',''],['step','step',''],['useMap','usemap',''],
        ]) reflectString(ip,prop,attr,def);
        for (const [prop,attr] of [
            ['defaultChecked','checked'],['formNoValidate','formnovalidate'],['incremental','incremental'],
            ['multiple','multiple'],['webkitdirectory','webkitdirectory'],
        ]) reflectBoolean(ip,prop,attr);
        for (const [prop,attr,def] of [
            ['height','height',0],['width','width',0],['size','size',20],['maxLength','maxlength',-1],['minLength','minlength',-1],
        ]) reflectNumber(ip,prop,attr,def);
        getter(ip,'value',function value(){ return state(this).value; },function value(v){
            const s=state(this); s.value=String(v); s.valueDirty=true; syncSelectorFormValue(this,s.value);
        });
        getter(ip,'checked',function checked(){ return state(this).checked; },function checked(v){ const s=state(this); s.checked=!!v; s.checkedDirty=true; });
        getter(ip,'defaultValue',function defaultValue(){ return this.getAttribute('value')||''; },function defaultValue(v){
            const text=String(v); const s=state(this); this.setAttribute('value',text);
            if(!s.valueDirty){s.value=text;syncSelectorFormValue(this,s.value);}
        });
        getter(ip,'defaultChecked',function defaultChecked(){ return this.hasAttribute('checked'); },function defaultChecked(v){
            const s=state(this); if(v)this.setAttribute('checked','');else this.removeAttribute('checked'); if(!s.checkedDirty)s.checked=!!v;
        });
        getter(ip,'files',function files(){ return state(this).files; },function files(v){ state(this).files=v==null?null:v; });
        getter(ip,'form',function form(){ return this.closest ? this.closest('form') : null; });
        getter(ip,'indeterminate',function indeterminate(){ return state(this).indeterminate; },function indeterminate(v){ state(this).indeterminate=!!v; });
        getter(ip,'labels',function labels(){
            const id=this.id; return id ? document.querySelectorAll(`label[for="${String(id).replace(/"/g,'\\"')}"]`) : document.querySelectorAll('label[for="__browser_oxide_none__"]');
        });
        getter(ip,'list',function list(){ const id=this.getAttribute('list'); return id?document.getElementById(id):null; });
        reflectString(ip,'popoverTargetAction','popovertargetaction','toggle');
        getter(ip,'popoverTargetElement',function popoverTargetElement(){ const id=this.getAttribute('popovertarget'); return id?document.getElementById(id):null; },function popoverTargetElement(v){ if(v&&v.id)this.setAttribute('popovertarget',v.id);else this.removeAttribute('popovertarget'); });
        getter(ip,'selectionStart',function selectionStart(){ return state(this).selectionStart; },function selectionStart(v){ state(this).selectionStart=Math.max(0,Number(v)||0); });
        getter(ip,'selectionEnd',function selectionEnd(){ return state(this).selectionEnd; },function selectionEnd(v){ state(this).selectionEnd=Math.max(0,Number(v)||0); });
        getter(ip,'selectionDirection',function selectionDirection(){ return state(this).selectionDirection; },function selectionDirection(v){ state(this).selectionDirection=['forward','backward','none'].includes(String(v))?String(v):'none'; });
        getter(ip,'validationMessage',function validationMessage(){ return state(this).customValidity; });
        getter(ip,'willValidate',function willValidate(){ return !this.disabled; });
        getter(ip,'validity',function validity(){
            const self=this;
            const target=Object.create(globalThis.ValidityState?.prototype||Object.prototype);
            return new Proxy(target,{get(_t,k){
                if(k==='valid')return !state(self).customValidity;
                if(k==='customError')return !!state(self).customValidity;
                if(['badInput','patternMismatch','rangeOverflow','rangeUnderflow','stepMismatch','tooLong','tooShort','typeMismatch','valueMissing'].includes(k))return false;
                return Reflect.get(target,k);
            }});
        });
        getter(ip,'webkitEntries',function webkitEntries(){ return []; });
        getter(ip,'valueAsNumber',function valueAsNumber(){ const n=Number(this.value); return this.value===''?NaN:n; },function valueAsNumber(v){ this.value=Number.isNaN(Number(v))?'':String(Number(v)); });
        getter(ip,'valueAsDate',function valueAsDate(){ const d=new Date(this.value); return Number.isNaN(d.getTime())?null:d; },function valueAsDate(v){ if(v==null){this.value='';return;} const d=new Date(v); this.value=Number.isNaN(d.getTime())?'':d.toISOString().slice(0,10); });
        method(ip,'checkValidity',function checkValidity(){ return !state(this).customValidity; },0);
        method(ip,'reportValidity',function reportValidity(){ return this.checkValidity(); },0);
        method(ip,'select',function select(){ const s=state(this); s.selectionStart=0; s.selectionEnd=String(this.value||'').length; s.selectionDirection='none'; },0);
        method(ip,'setCustomValidity',function setCustomValidity(error){ state(this).customValidity=String(error); },1);
        method(ip,'setSelectionRange',function setSelectionRange(start,end,direction='none'){ const s=state(this); s.selectionStart=Math.max(0,Number(start)||0); s.selectionEnd=Math.max(s.selectionStart,Number(end)||0); s.selectionDirection=['forward','backward','none'].includes(String(direction))?String(direction):'none'; },2);
        method(ip,'setRangeText',function setRangeText(replacement,start,end,selectionMode='preserve'){
            const s=state(this); const value=String(this.value||'');
            const a=arguments.length>=2?Math.max(0,Number(start)||0):s.selectionStart;
            const b=arguments.length>=3?Math.max(a,Number(end)||0):s.selectionEnd;
            this.value=value.slice(0,a)+String(replacement)+value.slice(b);
            const next=a+String(replacement).length;
            if(selectionMode==='select'){s.selectionStart=a;s.selectionEnd=next;}else if(selectionMode==='start'){s.selectionStart=s.selectionEnd=a;}else{s.selectionStart=s.selectionEnd=next;}
        },1);
        method(ip,'showPicker',function showPicker(){},0);
        method(ip,'stepUp',function stepUp(n=1){ const step=Number(this.step)||1; const cur=Number(this.value)||0; this.value=String(cur+step*(Number(n)||1)); },0);
        method(ip,'stepDown',function stepDown(n=1){ const step=Number(this.step)||1; const cur=Number(this.value)||0; this.value=String(cur-step*(Number(n)||1)); },0);
        makeEnumerable(ip);
    }

    // ---------------------------------------------------------------
    // HTMLTextAreaElement live value/defaultValue state
    // ---------------------------------------------------------------
    if (tp) {
        const textAreaState = new WeakMap();
        const state = self => {
            let s = textAreaState.get(self);
            if (!s) {
                s = { value: String(self.textContent || ''), valueDirty: false };
                textAreaState.set(self, s);
            }
            return s;
        };
        getter(tp,'value',function value(){ return state(this).value; },function value(v){
            const s=state(this); s.value=String(v); s.valueDirty=true; syncSelectorFormValue(this,s.value);
        });
        getter(tp,'defaultValue',function defaultValue(){ return String(this.textContent || ''); },function defaultValue(v){
            const text=String(v); const s=state(this); this.textContent=text;
            if(!s.valueDirty){s.value=text;syncSelectorFormValue(this,s.value);}
        });
        makeEnumerable(tp);
    }

    // ---------------------------------------------------------------
    // HTMLIFrameElement
    // ---------------------------------------------------------------
    if (fp) {
        for (const [prop,attr,def] of [
            ['align','align',''],['allow','allow',''],['csp','csp',''],['frameBorder','frameborder',''],
            ['height','height',''],['longDesc','longdesc',''],['marginHeight','marginheight',''],
            ['marginWidth','marginwidth',''],['scrolling','scrolling',''],['width','width',''],
        ]) reflectString(fp,prop,attr,def);
        getter(fp,'loading',function loading(){ return this.getAttribute('loading')||'eager'; },function loading(v){ this.setAttribute('loading',String(v)); });
        for (const [prop,attr] of [
            ['adAuctionHeaders','adauctionheaders'],['allowFullscreen','allowfullscreen'],
            ['allowPaymentRequest','allowpaymentrequest'],['browsingTopics','browsingtopics'],
            ['credentialless','credentialless'],['privateToken','privatetoken'],
            ['sharedStorageWritable','sharedstoragewritable'],
        ]) reflectBoolean(fp,prop,attr);
        if (!Object.prototype.hasOwnProperty.call(fp,'src') && elementDescriptors.src) {
            Object.defineProperty(fp,'src',{...elementDescriptors.src,enumerable:true});
        }
        if (!Object.prototype.hasOwnProperty.call(fp,'referrerPolicy') && elementDescriptors.referrerPolicy) {
            Object.defineProperty(fp,'referrerPolicy',{...elementDescriptors.referrerPolicy,enumerable:true});
        }
        getter(fp,'sandbox',function sandbox(){ return tokenListFor(this,'sandbox'); },function sandbox(v){ this.setAttribute('sandbox',String(v)); });
        const featurePolicies=new WeakMap();
        getter(fp,'featurePolicy',function featurePolicy(){ let v=featurePolicies.get(this); if(!v){v=Object.create(globalThis.FeaturePolicy?.prototype||Object.prototype);featurePolicies.set(this,v);} return v; });
        method(fp,'getSVGDocument',function getSVGDocument(){
            const doc=this.contentDocument; if(!doc)return null; const root=doc.documentElement; return root&&String(root.localName||'').toLowerCase()==='svg'?doc:null;
        },0);
        makeEnumerable(fp);
    }

    // Ensure all existing Element own WebIDL descriptors are enumerable after
    // relocation and no old HTML-only properties remain on Element.prototype.
    makeEnumerable(ep);
    makeEnumerable(hp);
    if (dp) makeEnumerable(dp);

    // Public DOM constructors are illegal in Chrome. Use wrappers that share
    // the original instance prototypes; internal lexical classes keep creating
    // nodes normally, while public `new` gets correct browser semantics.
    const ElementCtor = illegalWrapper('Element', ElementOriginal, globalThis.Node);
    const HTMLElementCtor = illegalWrapper('HTMLElement', HTMLElementOriginal, ElementCtor);
    illegalWrapper('HTMLDivElement', DivOriginal, HTMLElementCtor);
    illegalWrapper('HTMLInputElement', InputOriginal, HTMLElementCtor);
    illegalWrapper('HTMLIFrameElement', IFrameOriginal, HTMLElementCtor);
})(globalThis);
