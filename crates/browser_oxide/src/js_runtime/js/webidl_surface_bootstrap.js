// Low-risk WebIDL descriptor normalization for interfaces whose behavior is
// already implemented elsewhere. This layer intentionally avoids replacing
// object models or network/runtime semantics.
((globalThis) => {
    const setEnumerable = (prototype, names) => {
        for (const name of names) {
            const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
            if (!descriptor) continue;
            descriptor.enumerable = true;
            Object.defineProperty(prototype, name, descriptor);
        }
    };
    const setLength = (fn, length) => {
        if (typeof fn !== 'function') return;
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
    };
    const maskCtor = (Ctor, name) => {
        try { if (typeof _maskFunction === 'function' && typeof Ctor === 'function') _maskFunction(Ctor, name); } catch (_) {}
    };

    if (globalThis.DOMParser?.prototype) {
        maskCtor(globalThis.DOMParser, 'DOMParser');
        setEnumerable(globalThis.DOMParser.prototype, ['parseFromString']);
    }

    if (globalThis.MediaQueryList?.prototype) {
        const OriginalCtor = globalThis.MediaQueryList;
        const proto = OriginalCtor.prototype;
        const originalMatchMedia = globalThis.matchMedia;
        const matchesDescriptor = Object.getOwnPropertyDescriptor(proto, 'matches');
        const mediaDescriptor = Object.getOwnPropertyDescriptor(proto, 'media');
        const onchangeDescriptor = Object.getOwnPropertyDescriptor(proto, 'onchange');
        const states = new WeakMap();

        function MediaQueryList() {
            throw new TypeError("Failed to construct 'MediaQueryList': Illegal constructor");
        }
        try { Object.setPrototypeOf(MediaQueryList, globalThis.EventTarget); } catch (_) {}
        try { MediaQueryList.prototype = proto; } catch (_) {}
        setLength(MediaQueryList, 0);
        maskCtor(MediaQueryList, 'MediaQueryList');
        try {
            Object.defineProperty(proto, 'constructor', {
                value: MediaQueryList, writable: true, configurable: true, enumerable: false,
            });
        } catch (_) {}

        if (matchesDescriptor?.get) {
            Object.defineProperty(proto, 'matches', {
                get: function matches() {
                    const state = states.get(this);
                    return state ? state.matches : matchesDescriptor.get.call(this);
                },
                enumerable: true, configurable: true,
            });
        }
        if (mediaDescriptor?.get) {
            Object.defineProperty(proto, 'media', {
                get: function media() {
                    const state = states.get(this);
                    return state ? state.media : mediaDescriptor.get.call(this);
                },
                enumerable: true, configurable: true,
            });
        }
        if (onchangeDescriptor?.get) {
            Object.defineProperty(proto, 'onchange', {
                get: function onchange() {
                    const state = states.get(this);
                    return state ? state.onchange : onchangeDescriptor.get.call(this);
                },
                set: function onchange(value) {
                    const state = states.get(this);
                    if (state) {
                        state.onchange = typeof value === 'function' ? value : null;
                    } else if (onchangeDescriptor.set) {
                        onchangeDescriptor.set.call(this, value);
                    }
                },
                enumerable: true, configurable: true,
            });
        }

        if (typeof originalMatchMedia === 'function') {
            globalThis.matchMedia = function matchMedia(query) {
                const result = originalMatchMedia.call(globalThis, query);
                let state;
                try {
                    state = {
                        matches: matchesDescriptor?.get ? matchesDescriptor.get.call(result) : !!result.matches,
                        media: mediaDescriptor?.get ? mediaDescriptor.get.call(result) : String(result.media || ''),
                        onchange: onchangeDescriptor?.get ? onchangeDescriptor.get.call(result) : null,
                    };
                } catch (_) {
                    state = { matches: !!result.matches, media: String(result.media || ''), onchange: null };
                }
                states.set(result, state);
                try { delete result._matches; delete result._media; delete result._onchange; } catch (_) {}
                return result;
            };
            try { if (typeof _maskFunction === 'function') _maskFunction(globalThis.matchMedia, 'matchMedia'); } catch (_) {}
        }

        globalThis.MediaQueryList = MediaQueryList;
        setEnumerable(proto, ['matches', 'media', 'onchange', 'addListener', 'removeListener']);
    }

    if (globalThis.ResizeObserver?.prototype) {
        setEnumerable(globalThis.ResizeObserver.prototype, ['observe', 'unobserve', 'disconnect']);
    }

    if (globalThis.DOMTokenList?.prototype) {
        const Ctor = globalThis.DOMTokenList;
        const proto = Ctor.prototype;
        setLength(Ctor, 0);

        const proxyTargets = new WeakMap();
        const unwrap = value => proxyTargets.get(value) || value;

        const originalAdd = proto.add;
        const originalRemove = proto.remove;
        const originalContains = proto.contains;
        const originalValue = Object.getOwnPropertyDescriptor(proto, 'value');
        if (typeof originalAdd === 'function') {
            Object.defineProperty(proto, 'add', {
                value: function add() {
                    const target = unwrap(this);
                    for (const token of arguments) originalAdd.call(target, String(token));
                },
                writable: true, configurable: true, enumerable: true,
            });
        }
        if (typeof originalRemove === 'function') {
            Object.defineProperty(proto, 'remove', {
                value: function remove() {
                    const target = unwrap(this);
                    for (const token of arguments) originalRemove.call(target, String(token));
                },
                writable: true, configurable: true, enumerable: true,
            });
        }
        if (typeof originalContains === 'function') {
            Object.defineProperty(proto, 'contains', {
                value: function contains(token) {
                    return originalContains.call(unwrap(this), String(token));
                },
                writable: true, configurable: true, enumerable: true,
            });
        }

        if (originalValue?.get && !originalValue.set) {
            Object.defineProperty(proto, 'value', {
                get: function value() {
                    return originalValue.get.call(unwrap(this));
                },
                set: function value(value) {
                    const current = Array.from(this.values());
                    if (current.length) this.remove(...current);
                    const next = String(value).trim().split(/\s+/).filter(Boolean);
                    if (next.length) this.add(...next);
                },
                enumerable: true,
                configurable: true,
            });
        }

        if (typeof proto.replace !== 'function') {
            Object.defineProperty(proto, 'replace', {
                value: function replace(token, newToken) {
                    const oldValue = String(token);
                    const replacement = String(newToken);
                    const tokens = Array.from(this.values());
                    const index = tokens.indexOf(oldValue);
                    if (index < 0) return false;
                    tokens[index] = replacement;
                    this.value = tokens.join(' ');
                    return true;
                },
                writable: true, configurable: true, enumerable: true,
            });
        }
        if (typeof proto.supports !== 'function') {
            Object.defineProperty(proto, 'supports', {
                value: function supports() {
                    throw new TypeError('DOMTokenList has no supported tokens');
                },
                writable: true, configurable: true, enumerable: true,
            });
            setLength(proto.supports, 1);
        }

        setLength(proto.forEach, 1);
        setEnumerable(proto, [
            'length', 'value', 'item', 'contains', 'add', 'remove', 'toggle',
            'replace', 'supports', 'entries', 'keys', 'values', 'forEach', 'toString',
        ]);

        // DOMTokenList is a legacy indexed platform object. Keep the real
        // private-field-bearing target intact and expose live numeric own
        // properties through a stable Proxy returned by Element.classList.
        let classListOwner = globalThis.Element?.prototype || null;
        let classListDescriptor = null;
        while (classListOwner && !classListDescriptor) {
            classListDescriptor = Object.getOwnPropertyDescriptor(classListOwner, 'classList');
            if (!classListDescriptor) classListOwner = Object.getPrototypeOf(classListOwner);
        }
        if (classListOwner && classListDescriptor?.get) {
            const originalClassListGet = classListDescriptor.get;
            const cache = new WeakMap();
            const isIndex = prop => typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop);
            Object.defineProperty(classListOwner, 'classList', {
                get: function classList() {
                    const cached = cache.get(this);
                    if (cached) return cached;
                    const target = originalClassListGet.call(this);
                    const proxy = new Proxy(target, {
                        get(object, prop, receiver) {
                            if (isIndex(prop)) {
                                const index = Number(prop);
                                return index < object.length ? object.item(index) : undefined;
                            }
                            return Reflect.get(object, prop, receiver);
                        },
                        has(object, prop) {
                            if (isIndex(prop)) return Number(prop) < object.length;
                            return Reflect.has(object, prop);
                        },
                        ownKeys(object) {
                            const numeric = Array.from({ length: object.length }, (_, index) => String(index));
                            const rest = Reflect.ownKeys(object).filter(key => !isIndex(key));
                            return [...numeric, ...rest];
                        },
                        getOwnPropertyDescriptor(object, prop) {
                            if (isIndex(prop)) {
                                const index = Number(prop);
                                if (index >= object.length) return undefined;
                                return {
                                    value: object.item(index),
                                    writable: false,
                                    enumerable: true,
                                    configurable: true,
                                };
                            }
                            return Reflect.getOwnPropertyDescriptor(object, prop);
                        },
                        set(object, prop, value, receiver) {
                            if (isIndex(prop)) return false;
                            return Reflect.set(object, prop, value, receiver);
                        },
                    });
                    proxyTargets.set(proxy, target);
                    cache.set(this, proxy);
                    return proxy;
                },
                enumerable: classListDescriptor.enumerable,
                configurable: classListDescriptor.configurable,
            });
        }
    }
})(globalThis);
