// Attr + NamedNodeMap WebIDL layer built on Element's existing attribute API.
// The DOM backend stores attributes by qualified name; this layer provides the
// live object identity, Node inheritance and legacy indexed/named properties
// that Chromium exposes without changing the underlying storage model.
((globalThis) => {
    if (!globalThis.document || !globalThis.Element || !globalThis.Node) return;

    const attrState = new WeakMap();
    const mapState = new WeakMap();
    const elementAttrCache = new WeakMap();
    const elementMapCache = new WeakMap();

    const splitName = name => {
        const qname = String(name);
        const index = qname.indexOf(':');
        return index >= 0
            ? { name:qname, prefix:qname.slice(0,index), localName:qname.slice(index+1) }
            : { name:qname, prefix:null, localName:qname };
    };
    const stateForAttr = value => {
        const state = attrState.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const stateForMap = value => {
        const state = mapState.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const domException = (message, name) => {
        try { return new DOMException(message, name); }
        catch (_) { const error = new Error(message); error.name = name; return error; }
    };

    function Attr() {
        throw new TypeError("Failed to construct 'Attr': Illegal constructor");
    }
    Attr.prototype = Object.create(globalThis.Node.prototype);
    Object.defineProperty(Attr.prototype, 'constructor', {
        value: Attr, writable: true, configurable: true,
    });

    const syncAttr = state => {
        if (state.owner) {
            let present = false;
            try { present = state.owner.hasAttribute(state.name); } catch (_) {}
            if (present) {
                try { state.value = String(state.owner.getAttribute(state.name) ?? ''); } catch (_) {}
            } else {
                state.owner = null;
            }
        }
        return state;
    };
    const attrValue = state => syncAttr(state).value;
    const setAttrValue = (state, value) => {
        const stringValue = String(value);
        state.value = stringValue;
        if (state.owner) {
            if (state.namespaceURI != null && typeof state.owner.setAttributeNS === 'function') {
                state.owner.setAttributeNS(state.namespaceURI, state.name, stringValue);
            } else {
                state.owner.setAttribute(state.name, stringValue);
            }
        }
    };
    const emptyChildren = () => {
        try { return document.createElement('span').childNodes; }
        catch (_) { return []; }
    };

    const makeAttr = (name, value = '', owner = null, namespaceURI = null) => {
        const parts = splitName(name);
        const target = Object.create(Attr.prototype);
        const state = {
            owner,
            name: parts.name,
            prefix: parts.prefix,
            localName: parts.localName,
            namespaceURI: namespaceURI == null ? null : String(namespaceURI),
            value: String(value ?? ''),
            childNodes: null,
        };
        const proxy = new Proxy(target, {
            get(object, prop, receiver) {
                const s = syncAttr(state);
                switch (prop) {
                    case 'nodeType': return 2;
                    case 'nodeName': return s.name;
                    case 'nodeValue': return attrValue(s);
                    case 'textContent': return attrValue(s);
                    case 'ownerDocument': return globalThis.document;
                    case 'parentNode':
                    case 'parentElement':
                    case 'previousSibling':
                    case 'nextSibling':
                    case 'firstChild':
                    case 'lastChild': return null;
                    case 'isConnected': return false;
                    case 'baseURI': return globalThis.document?.baseURI || globalThis.location?.href || 'about:blank';
                    case 'childNodes':
                        if (!s.childNodes) s.childNodes = emptyChildren();
                        return s.childNodes;
                    case 'hasChildNodes': return function hasChildNodes() { return false; };
                    case 'contains': return function contains(other) { return other === proxy; };
                    case 'isSameNode': return function isSameNode(other) { return other === proxy; };
                    case 'isEqualNode': return function isEqualNode(other) {
                        if (!(other instanceof Attr)) return false;
                        return other.name === proxy.name && other.value === proxy.value
                            && other.namespaceURI === proxy.namespaceURI;
                    };
                    case 'cloneNode': return function cloneNode() {
                        return makeAttr(s.name, attrValue(s), null, s.namespaceURI);
                    };
                    case 'getRootNode': return function getRootNode() { return proxy; };
                    case 'normalize': return function normalize() {};
                    default: return Reflect.get(object, prop, receiver);
                }
            },
            set(object, prop, value, receiver) {
                if (prop === 'nodeValue' || prop === 'textContent') {
                    setAttrValue(state, value);
                    return true;
                }
                return Reflect.set(object, prop, value, receiver);
            },
        });
        attrState.set(target, state);
        attrState.set(proxy, state);
        return proxy;
    };

    const attrAccessors = {
        namespaceURI: state => syncAttr(state).namespaceURI,
        prefix: state => syncAttr(state).prefix,
        localName: state => syncAttr(state).localName,
        name: state => syncAttr(state).name,
        specified: () => true,
        ownerElement: state => syncAttr(state).owner,
        value: state => attrValue(state),
    };
    for (const [name, getter] of Object.entries(attrAccessors)) {
        const descriptor = {
            get: function () { return getter(stateForAttr(this)); },
            enumerable: true,
            configurable: true,
        };
        if (name === 'value') {
            descriptor.set = function (value) { setAttrValue(stateForAttr(this), value); };
        }
        Object.defineProperty(Attr.prototype, name, descriptor);
    }
    Object.defineProperty(Attr.prototype, Symbol.toStringTag, { value:'Attr', configurable:true });

    function NamedNodeMap() {
        throw new TypeError("Failed to construct 'NamedNodeMap': Illegal constructor");
    }
    const originalAttributesOwner = (() => {
        let owner = globalThis.Element.prototype;
        while (owner) {
            const descriptor = Object.getOwnPropertyDescriptor(owner, 'attributes');
            if (descriptor?.get) return { owner, descriptor };
            owner = Object.getPrototypeOf(owner);
        }
        return null;
    })();
    const rawAttributes = element => originalAttributesOwner.descriptor.get.call(element);
    const attributeNames = element => {
        const raw = rawAttributes(element);
        const names = [];
        for (let index = 0; index < Number(raw.length || 0); index++) {
            const item = raw.item(index);
            if (item?.name != null) names.push(String(item.name));
        }
        return names;
    };
    const cacheFor = element => {
        let cache = elementAttrCache.get(element);
        if (!cache) { cache = new Map(); elementAttrCache.set(element, cache); }
        return cache;
    };
    const attachedAttr = (element, name, namespaceURI = null) => {
        if (!element.hasAttribute(name)) return null;
        const cache = cacheFor(element);
        const existing = cache.get(name);
        if (existing) {
            const state = syncAttr(stateForAttr(existing));
            if (state.owner === element) return existing;
            cache.delete(name);
        }
        const attr = makeAttr(name, element.getAttribute(name) ?? '', element, namespaceURI);
        cache.set(name, attr);
        return attr;
    };
    const detachAttr = attr => {
        const state = stateForAttr(attr);
        state.value = attrValue(state);
        state.owner = null;
        return attr;
    };
    const findByNS = (map, namespaceURI, localName) => {
        const state = stateForMap(map);
        const ns = namespaceURI == null ? null : String(namespaceURI);
        const local = String(localName);
        for (const name of attributeNames(state.element)) {
            const attr = attachedAttr(state.element, name);
            if (attr && attr.localName === local && attr.namespaceURI === ns) return attr;
        }
        return null;
    };

    Object.defineProperty(NamedNodeMap.prototype, 'length', {
        get: function length() { return attributeNames(stateForMap(this).element).length; },
        enumerable: true, configurable: true,
    });
    const mapMethods = {
        item(index) {
            const state = stateForMap(this);
            const names = attributeNames(state.element);
            const name = names[Number(index) >>> 0];
            return name === undefined ? null : attachedAttr(state.element, name);
        },
        getNamedItem(name) {
            const state = stateForMap(this);
            return attachedAttr(state.element, String(name));
        },
        getNamedItemNS(namespaceURI, localName) {
            return findByNS(this, namespaceURI, localName);
        },
        setNamedItem(attr) {
            const state = stateForMap(this);
            if (!(attr instanceof Attr) || !attrState.has(attr)) {
                throw new TypeError("Failed to execute 'setNamedItem' on 'NamedNodeMap': parameter 1 is not of type 'Attr'.");
            }
            const attrInfo = stateForAttr(attr);
            syncAttr(attrInfo);
            if (attrInfo.owner && attrInfo.owner !== state.element) {
                throw domException('The attribute is in use.', 'InUseAttributeError');
            }
            const previous = this.getNamedItem(attrInfo.name);
            const value = attrValue(attrInfo);
            if (previous && previous !== attr) detachAttr(previous);
            state.element.setAttribute(attrInfo.name, value);
            attrInfo.owner = state.element;
            cacheFor(state.element).set(attrInfo.name, attr);
            return previous === attr ? attr : previous;
        },
        setNamedItemNS(attr) { return this.setNamedItem(attr); },
        removeNamedItem(name) {
            const state = stateForMap(this);
            const key = String(name);
            const attr = this.getNamedItem(key);
            if (!attr) throw domException(`No item with name '${key}' was found.`, 'NotFoundError');
            const value = attr.value;
            state.element.removeAttribute(key);
            const attrInfo = stateForAttr(attr);
            attrInfo.value = value;
            attrInfo.owner = null;
            return attr;
        },
        removeNamedItemNS(namespaceURI, localName) {
            const attr = findByNS(this, namespaceURI, localName);
            if (!attr) throw domException(`No item with local name '${localName}' was found.`, 'NotFoundError');
            return this.removeNamedItem(attr.name);
        },
    };
    const methodLengths = {
        item:1, getNamedItem:1, getNamedItemNS:2, setNamedItem:1,
        setNamedItemNS:1, removeNamedItem:1, removeNamedItemNS:2,
    };
    for (const [name, method] of Object.entries(mapMethods)) {
        Object.defineProperty(method, 'length', { value:methodLengths[name], configurable:true });
        Object.defineProperty(NamedNodeMap.prototype, name, {
            value:method, writable:true, enumerable:true, configurable:true,
        });
    }
    const values = function values() {
        const map = this;
        let index = 0;
        return {
            next() {
                const value = map.item(index++);
                return value === null ? { value:undefined, done:true } : { value, done:false };
            },
            [Symbol.iterator]() { return this; },
        };
    };
    Object.defineProperty(NamedNodeMap.prototype, Symbol.iterator, {
        value:values, writable:true, configurable:true,
    });
    Object.defineProperty(NamedNodeMap.prototype, Symbol.toStringTag, { value:'NamedNodeMap', configurable:true });

    const makeMap = element => {
        const cached = elementMapCache.get(element);
        if (cached) return cached;
        const target = Object.create(NamedNodeMap.prototype);
        const proxy = new Proxy(target, {
            get(object, prop, receiver) {
                const state = stateForMap(receiver);
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    return receiver.item(Number(prop));
                }
                if (typeof prop === 'string' && !Reflect.has(object, prop)) {
                    const attr = attachedAttr(state.element, prop);
                    if (attr) return attr;
                }
                return Reflect.get(object, prop, receiver);
            },
            has(object, prop) {
                const state = stateForMap(proxy);
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    return Number(prop) < attributeNames(state.element).length;
                }
                if (typeof prop === 'string' && state.element.hasAttribute(prop)) return true;
                return Reflect.has(object, prop);
            },
            ownKeys() {
                const state = stateForMap(proxy);
                const names = attributeNames(state.element);
                return [...names.map((_, index) => String(index)), ...names];
            },
            getOwnPropertyDescriptor(object, prop) {
                const state = stateForMap(proxy);
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    const attr = proxy.item(Number(prop));
                    return attr === null ? undefined : {
                        value:attr, writable:false, enumerable:true, configurable:true,
                    };
                }
                if (typeof prop === 'string' && !Reflect.has(object, prop)) {
                    const attr = attachedAttr(state.element, prop);
                    if (attr) return { value:attr, writable:false, enumerable:false, configurable:true };
                }
                return Reflect.getOwnPropertyDescriptor(object, prop);
            },
            set(object, prop, value, receiver) {
                if (typeof prop === 'string' && (/^(0|[1-9]\d*)$/.test(prop) || stateForMap(proxy).element.hasAttribute(prop))) {
                    return false;
                }
                return Reflect.set(object, prop, value, receiver);
            },
        });
        mapState.set(target, { element });
        mapState.set(proxy, { element });
        elementMapCache.set(element, proxy);
        return proxy;
    };

    globalThis.Attr = Attr;
    globalThis.NamedNodeMap = NamedNodeMap;

    if (originalAttributesOwner) {
        Object.defineProperty(originalAttributesOwner.owner, 'attributes', {
            get:function attributes() { return makeMap(this); },
            enumerable:originalAttributesOwner.descriptor.enumerable,
            configurable:originalAttributesOwner.descriptor.configurable,
        });
    }

    const docProto = globalThis.Document?.prototype;
    if (docProto) {
        Object.defineProperty(docProto, 'createAttribute', {
            value:function createAttribute(localName) {
                return makeAttr(String(localName).toLowerCase(), '', null, null);
            },
            writable:true, enumerable:true, configurable:true,
        });
        Object.defineProperty(docProto, 'createAttributeNS', {
            value:function createAttributeNS(namespaceURI, qualifiedName) {
                return makeAttr(String(qualifiedName), '', null, namespaceURI == null ? null : String(namespaceURI));
            },
            writable:true, enumerable:true, configurable:true,
        });
    }

    const elementProto = globalThis.Element?.prototype;
    if (elementProto) {
        const elementMethods = {
            getAttributeNode(name) { return this.attributes.getNamedItem(String(name)); },
            getAttributeNodeNS(namespaceURI, localName) { return this.attributes.getNamedItemNS(namespaceURI, localName); },
            setAttributeNode(attr) { return this.attributes.setNamedItem(attr); },
            setAttributeNodeNS(attr) { return this.attributes.setNamedItemNS(attr); },
            removeAttributeNode(attr) {
                if (!(attr instanceof Attr) || attr.ownerElement !== this) {
                    throw domException('The node provided is not an attribute of this element.', 'NotFoundError');
                }
                return this.attributes.removeNamedItem(attr.name);
            },
        };
        for (const [name, method] of Object.entries(elementMethods)) {
            const lengths = { getAttributeNode:1, getAttributeNodeNS:2, setAttributeNode:1, setAttributeNodeNS:1, removeAttributeNode:1 };
            Object.defineProperty(method, 'length', { value:lengths[name], configurable:true });
            Object.defineProperty(elementProto, name, {
                value:method, writable:true, enumerable:true, configurable:true,
            });
        }
    }

    try {
        if (typeof _maskFunction === 'function') {
            _maskFunction(Attr, 'Attr');
            _maskFunction(NamedNodeMap, 'NamedNodeMap');
            _maskFunction(values, 'values');
        }
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(Attr.prototype, 'namespaceURI','prefix','localName','name','specified','ownerElement','value');
            _maskAsNative(NamedNodeMap.prototype, ...Object.keys(mapMethods), 'length');
            if (docProto) _maskAsNative(docProto, 'createAttribute','createAttributeNS');
            if (elementProto) _maskAsNative(elementProto, 'getAttributeNode','getAttributeNodeNS','setAttributeNode','setAttributeNodeNS','removeAttributeNode','attributes');
        }
    } catch (_) {}
})(globalThis);
