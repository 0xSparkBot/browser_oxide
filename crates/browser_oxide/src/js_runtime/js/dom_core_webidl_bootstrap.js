// Deep WebIDL normalization for the DOM core hierarchy.  The DOM backend and
// node identity remain owned by dom_bootstrap; this layer only repairs the
// public prototype surface and fills standard mixin behavior.
((globalThis) => {
    const mask = (fn, name) => {
        if (typeof fn !== 'function') return fn;
        try {
            Object.defineProperty(fn, 'name', { value: name || fn.name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') {
                globalThis._maskFunction(fn, name || fn.name);
            }
        } catch (_) {}
        return fn;
    };
    const setLength = (fn, length) => {
        if (typeof fn !== 'function') return;
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
    };
    const method = (proto, name, fn, length) => {
        setLength(fn, length);
        mask(fn, name);
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
    const makeEnumerable = (proto, names) => {
        for (const name of names) {
            const d = Object.getOwnPropertyDescriptor(proto, name);
            if (!d || !d.configurable) continue;
            d.enumerable = true;
            Object.defineProperty(proto, name, d);
        }
    };

    // ---------------------------------------------------------------
    // Node
    // ---------------------------------------------------------------
    if (globalThis.Node?.prototype) {
        const OriginalNode = globalThis.Node;
        const proto = OriginalNode.prototype;
        function Node() {
            throw new TypeError('Illegal constructor');
        }
        try { Object.setPrototypeOf(Node, Object.getPrototypeOf(OriginalNode)); } catch (_) {}
        try { Node.prototype = proto; } catch (_) {}
        try {
            Object.defineProperty(proto, 'constructor', {
                value: Node, writable: true, configurable: true, enumerable: false,
            });
        } catch (_) {}
        // Keep already-defined DOM class constructors on their original
        // lexical superclass; changing their [[Prototype]] would also change
        // the target of `super()` at construction time.
        globalThis.Node = Node;
        const Ctor = Node;
        const constants = {
            ELEMENT_NODE: 1,
            ATTRIBUTE_NODE: 2,
            TEXT_NODE: 3,
            CDATA_SECTION_NODE: 4,
            ENTITY_REFERENCE_NODE: 5,
            ENTITY_NODE: 6,
            PROCESSING_INSTRUCTION_NODE: 7,
            COMMENT_NODE: 8,
            DOCUMENT_NODE: 9,
            DOCUMENT_TYPE_NODE: 10,
            DOCUMENT_FRAGMENT_NODE: 11,
            NOTATION_NODE: 12,
            DOCUMENT_POSITION_DISCONNECTED: 1,
            DOCUMENT_POSITION_PRECEDING: 2,
            DOCUMENT_POSITION_FOLLOWING: 4,
            DOCUMENT_POSITION_CONTAINS: 8,
            DOCUMENT_POSITION_CONTAINED_BY: 16,
            DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32,
        };
        for (const [name, value] of Object.entries(constants)) {
            for (const target of [Ctor, proto]) {
                const d = Object.getOwnPropertyDescriptor(target, name);
                if (!d || d.configurable) {
                    try {
                        Object.defineProperty(target, name, {
                            value, writable: false, enumerable: true, configurable: false,
                        });
                    } catch (_) {}
                }
            }
        }

        const elementForNamespaceLookup = node => {
            if (!node) return null;
            if (node.nodeType === 9) return node.documentElement || null;
            if (node.nodeType === 1) return node;
            if (node.nodeType === 2) return node.ownerElement || null;
            return node.parentElement || node.parentNode || null;
        };
        const lookupNamespace = (node, prefix) => {
            const wanted = prefix == null ? null : String(prefix);
            let el = elementForNamespaceLookup(node);
            while (el && el.nodeType === 1) {
                if (wanted === 'xml') return 'http://www.w3.org/XML/1998/namespace';
                if (wanted === 'xmlns') return 'http://www.w3.org/2000/xmlns/';
                try {
                    if (wanted === null) {
                        const declared = el.getAttribute && el.getAttribute('xmlns');
                        if (declared != null && declared !== '') return declared;
                        if (el.prefix == null && el.namespaceURI) return el.namespaceURI;
                        // HTML documents have the XHTML namespace even when
                        // the lightweight DOM backend does not persist an
                        // explicit namespace on parsed HTML elements.
                        const local = String(el.localName || el.tagName || '').toLowerCase();
                        if (el.ownerDocument === globalThis.document &&
                            (local === 'html' || el.ownerDocument?.documentElement === el)) {
                            return 'http://www.w3.org/1999/xhtml';
                        }
                    } else {
                        const declared = el.getAttribute && el.getAttribute(`xmlns:${wanted}`);
                        if (declared != null && declared !== '') return declared;
                        if (el.prefix === wanted && el.namespaceURI) return el.namespaceURI;
                    }
                } catch (_) {}
                el = el.parentElement;
            }
            return null;
        };
        method(proto, 'lookupNamespaceURI', function lookupNamespaceURI(prefix) {
            return lookupNamespace(this, prefix);
        }, 1);
        method(proto, 'lookupPrefix', function lookupPrefix(namespace) {
            const wanted = namespace == null ? null : String(namespace);
            if (!wanted) return null;
            if (wanted === 'http://www.w3.org/XML/1998/namespace') return 'xml';
            if (wanted === 'http://www.w3.org/2000/xmlns/') return 'xmlns';
            let el = elementForNamespaceLookup(this);
            while (el && el.nodeType === 1) {
                if (el.namespaceURI === wanted && el.prefix) return el.prefix;
                try {
                    const attrs = el.attributes;
                    for (let i = 0; attrs && i < attrs.length; i++) {
                        const attr = attrs.item(i);
                        if (attr && attr.name && attr.name.startsWith('xmlns:') && attr.value === wanted) {
                            return attr.name.slice(6);
                        }
                    }
                } catch (_) {}
                el = el.parentElement;
            }
            return null;
        }, 1);
        method(proto, 'isDefaultNamespace', function isDefaultNamespace(namespace) {
            const wanted = namespace == null ? null : String(namespace);
            return lookupNamespace(this, null) === wanted;
        }, 1);

        makeEnumerable(proto, Object.getOwnPropertyNames(proto).filter(name => name !== 'constructor'));
        mask(Ctor, 'Node');
    }

    // ---------------------------------------------------------------
    // DocumentFragment / ParentNode mixin
    // ---------------------------------------------------------------
    if (globalThis.DocumentFragment?.prototype) {
        const proto = globalThis.DocumentFragment.prototype;
        const oldScoped = proto._scopedElementIds;
        const oldQuery = proto.querySelector;
        const oldQueryAll = proto.querySelectorAll;
        const oldById = proto.getElementById;

        // Preserve the DOM bootstrap's optimized lexical NodeList path while
        // removing the page-observable private helper from the prototype.
        const receiverFor = target => new Proxy(target, {
            get(object, key, receiver) {
                if (key === '_scopedElementIds') return oldScoped.bind(object);
                return Reflect.get(object, key, receiver);
            },
        });
        if (typeof oldQuery === 'function') {
            method(proto, 'querySelector', function querySelector(selector) {
                return oldQuery.call(receiverFor(this), selector);
            }, 1);
        }
        if (typeof oldQueryAll === 'function') {
            method(proto, 'querySelectorAll', function querySelectorAll(selector) {
                return oldQueryAll.call(receiverFor(this), selector);
            }, 1);
        }
        if (typeof oldById === 'function') {
            method(proto, 'getElementById', function getElementById(id) {
                return oldById.call(receiverFor(this), id);
            }, 1);
        }
        try { delete proto._scopedElementIds; } catch (_) {}

        const toNode = value => value && typeof value === 'object' && typeof value.nodeType === 'number'
            ? value
            : globalThis.document.createTextNode(String(value));
        method(proto, 'append', function append() {
            for (const value of arguments) this.appendChild(toNode(value));
        }, 0);
        method(proto, 'prepend', function prepend() {
            const reference = this.firstChild;
            for (const value of arguments) this.insertBefore(toNode(value), reference);
        }, 0);
        method(proto, 'replaceChildren', function replaceChildren() {
            while (this.firstChild) this.removeChild(this.firstChild);
            for (const value of arguments) this.appendChild(toNode(value));
        }, 0);
        method(proto, 'moveBefore', function moveBefore(movedNode, referenceNode) {
            if (arguments.length < 2) {
                throw new TypeError("Failed to execute 'moveBefore' on 'DocumentFragment': 2 arguments required");
            }
            this.insertBefore(movedNode, referenceNode == null ? null : referenceNode);
        }, 2);

        makeEnumerable(proto, Object.getOwnPropertyNames(proto).filter(name => name !== 'constructor'));
        mask(globalThis.DocumentFragment, 'DocumentFragment');
    }

    // ---------------------------------------------------------------
    // ShadowRoot
    // ---------------------------------------------------------------
    if (globalThis.ShadowRoot?.prototype) {
        const proto = globalThis.ShadowRoot.prototype;
        const handlers = new WeakMap();
        getter(proto, 'activeElement', function activeElement() { return null; });
        getter(proto, 'clonable', function clonable() { return false; });
        getter(proto, 'customElementRegistry', function customElementRegistry() {
            return globalThis.customElements || null;
        });
        getter(proto, 'delegatesFocus', function delegatesFocus() { return false; });
        method(proto, 'elementFromPoint', function elementFromPoint() { return null; }, 2);
        method(proto, 'elementsFromPoint', function elementsFromPoint() { return []; }, 2);
        getter(proto, 'fullscreenElement', function fullscreenElement() { return null; }, function fullscreenElement() {});
        method(proto, 'getAnimations', function getAnimations() { return []; }, 0);
        method(proto, 'getHTML', function getHTML() { return this.innerHTML; }, 0);
        method(proto, 'getSelection', function getSelection() {
            return globalThis.getSelection ? globalThis.getSelection() : (globalThis.document?.getSelection?.() || null);
        }, 0);
        getter(proto, 'onslotchange', function onslotchange() {
            return handlers.get(this) || null;
        }, function onslotchange(value) {
            handlers.set(this, typeof value === 'function' ? value : null);
        });
        getter(proto, 'pictureInPictureElement', function pictureInPictureElement() { return null; });
        getter(proto, 'pointerLockElement', function pointerLockElement() { return null; });
        getter(proto, 'serializable', function serializable() { return false; });
        method(proto, 'setHTML', function setHTML(html) { this.innerHTML = String(html); }, 1);
        method(proto, 'setHTMLUnsafe', function setHTMLUnsafe(html) { this.innerHTML = String(html); }, 1);
        getter(proto, 'slotAssignment', function slotAssignment() { return 'named'; });
        getter(proto, 'styleSheets', function styleSheets() {
            const out = [];
            try {
                const styles = this.querySelectorAll('style,link[rel="stylesheet"]');
                for (let i = 0; i < styles.length; i++) {
                    if (styles[i] && styles[i].sheet) out.push(styles[i].sheet);
                }
            } catch (_) {}
            return out;
        });

        try {
            Object.defineProperty(proto, Symbol.toStringTag, {
                value: 'ShadowRoot', configurable: true,
            });
        } catch (_) {}
        makeEnumerable(proto, Object.getOwnPropertyNames(proto).filter(name => name !== 'constructor'));
        mask(globalThis.ShadowRoot, 'ShadowRoot');
    }
})(globalThis);
