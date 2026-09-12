// Custom Elements behavior layer.
//
// The DOM bootstrap owns node wrappers and mutation hooks; constructor WebIDL
// normalization owns the public HTML*Element constructors.  This layer joins
// the two with the construction-stack semantics required by HTML custom
// elements, while keeping all registry/upgrade state in WeakMaps/Sets so page
// reflection never sees browser_oxide implementation fields.
((globalThis) => {
    const ops = globalThis.Deno && Deno.core && Deno.core.ops;
    const construction = globalThis.__oxideCustomElementConstruction;
    const installDomHooks = globalThis.__oxideInstallCustomElementHooks;
    if (!construction || typeof construction.constructExisting !== 'function'
        || typeof installDomHooks !== 'function') return;

    const mask = (fn, name, length) => {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (length !== undefined) {
                Object.defineProperty(fn, 'length', { value: length, configurable: true });
            }
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };

    const registryStates = new WeakMap();
    const upgraded = new WeakSet();
    const failed = new WeakSet();
    const elementDefinitions = new WeakMap();

    const createRegistryState = () => ({
        definitions: new Map(),
        constructors: new Map(),
        pending: new Map(),
        initializedRoot: null,
    });

    const requireRegistry = value => {
        const state = registryStates.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };

    const requireArgument = (method, args, count = 1) => {
        if (args.length >= count) return;
        throw new TypeError(
            `Failed to execute '${method}' on 'CustomElementRegistry': ${count} argument required, but only ${args.length} present.`
        );
    };

    const reservedNames = new Set([
        'annotation-xml', 'color-profile', 'font-face', 'font-face-src',
        'font-face-uri', 'font-face-format', 'font-face-name', 'missing-glyph',
    ]);
    const isValidName = name => {
        if (reservedNames.has(name)) return false;
        // HTML potential-custom-element-name, covering the ASCII names used by
        // web frameworks. Uppercase is deliberately rejected.
        return /^[a-z][.0-9_a-z-]*-[.0-9_a-z-]*$/.test(name);
    };
    const syntaxError = (method, name) => new DOMException(
        `Failed to execute '${method}' on 'CustomElementRegistry': "${name}" is not a valid custom element name`,
        'SyntaxError'
    );
    const duplicateNameError = name => new DOMException(
        `Failed to execute 'define' on 'CustomElementRegistry': the name "${name}" has already been used with this registry`,
        'NotSupportedError'
    );
    const duplicateConstructorError = () => new DOMException(
        "Failed to execute 'define' on 'CustomElementRegistry': this constructor has already been used with this registry",
        'NotSupportedError'
    );

    const isConstructible = fn => {
        if (typeof fn !== 'function') return false;
        try { Reflect.construct(String, [], fn); return true; } catch (_) { return false; }
    };

    function CustomElementRegistry() {
        if (!new.target) {
            throw new TypeError(
                "Failed to construct 'CustomElementRegistry': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        }
        const instance = Object.create(CustomElementRegistry.prototype);
        registryStates.set(instance, createRegistryState());
        return instance;
    }
    mask(CustomElementRegistry, 'CustomElementRegistry', 0);

    const proto = CustomElementRegistry.prototype;
    try { delete proto.constructor; } catch (_) {}
    const method = (name, fn, length) => {
        mask(fn, name, length);
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };

    const definitionForElement = element => elementDefinitions.get(element) || null;
    const observedAttributes = entry => entry.observedAttributes;

    const callLifecycle = (element, methodName, args = []) => {
        const callback = element && element[methodName];
        if (typeof callback !== 'function') return;
        try { callback.apply(element, args); } catch (error) { console.error(error); }
    };

    const replayInitialAttributes = (element, entry) => {
        if (typeof element.attributeChangedCallback !== 'function') return;
        for (const name of observedAttributes(entry)) {
            if (!element.hasAttribute(name)) continue;
            callLifecycle(element, 'attributeChangedCallback', [name, null, element.getAttribute(name)]);
        }
    };

    const upgradeElement = (element, entry) => {
        if (!element || upgraded.has(element) || failed.has(element)) return element;
        elementDefinitions.set(element, entry);
        try {
            construction.constructExisting(entry.constructor, element);
            upgraded.add(element);
            replayInitialAttributes(element, entry);
            if (element.isConnected) callLifecycle(element, 'connectedCallback');
        } catch (error) {
            failed.add(element);
            console.error(error);
        }
        return element;
    };

    const elementMatchesEntry = (element, entry) => {
        if (!element || element.nodeType !== 1) return false;
        const localName = String(element.localName || element.tagName || '').toLowerCase();
        if (entry.extends) {
            return localName === entry.extends
                && String(element.getAttribute?.('is') || '').toLowerCase() === entry.name;
        }
        return localName === entry.name;
    };

    const upgradeTree = (root, state) => {
        if (!root) return;
        const candidates = [];
        if (root.nodeType === 1) candidates.push(root);
        if (typeof root.querySelectorAll === 'function') {
            const descendants = root.querySelectorAll('*');
            for (let i = 0; i < descendants.length; i++) candidates.push(descendants[i]);
        }
        for (const element of candidates) {
            if (upgraded.has(element) || failed.has(element)) continue;
            for (const entry of state.definitions.values()) {
                if (elementMatchesEntry(element, entry)) {
                    upgradeElement(element, entry);
                    break;
                }
            }
        }
    };

    method('define', function define(name, constructor, options) {
        requireArgument('define', arguments, 2);
        const state = requireRegistry(this);
        const normalized = String(name);
        if (!isValidName(normalized)) throw syntaxError('define', normalized);
        if (!isConstructible(constructor)) {
            throw new TypeError("Failed to execute 'define' on 'CustomElementRegistry': parameter 2 is not a constructor.");
        }
        if (state.definitions.has(normalized)) throw duplicateNameError(normalized);
        if (state.constructors.has(constructor)) throw duplicateConstructorError();

        let extendsTag = null;
        if (options !== undefined && options !== null && options.extends !== undefined) {
            extendsTag = String(options.extends).toLowerCase();
            if (extendsTag.includes('-')) {
                throw new DOMException(
                    `Failed to execute 'define' on 'CustomElementRegistry': "${extendsTag}" is an invalid local name for the extends option`,
                    'NotSupportedError'
                );
            }
        }

        let observed = [];
        if (constructor.prototype && typeof constructor.prototype.attributeChangedCallback === 'function') {
            const raw = constructor.observedAttributes;
            if (raw != null) observed = Array.from(raw, value => String(value).toLowerCase());
        }
        const entry = {
            name: normalized,
            constructor,
            extends: extendsTag,
            observedAttributes: observed,
        };
        state.definitions.set(normalized, entry);
        state.constructors.set(constructor, normalized);

        // Keep the Rust DOM selector matcher in sync with the document's
        // global registry so Selectors Level 4 `:defined` changes immediately
        // for already-parsed autonomous custom elements. Scoped registries are
        // intentionally not promoted into document-global selector state.
        if (this === globalRegistry && ops && typeof ops.op_dom_define_custom_element === 'function') {
            try { ops.op_dom_define_custom_element(normalized); } catch (_) {}
        }

        // The document uses the global registry. Upgrade already-parsed
        // candidates synchronously, before whenDefined reactions run.
        if (this === globalRegistry && globalThis.document) {
            upgradeTree(globalThis.document, state);
        } else if (state.initializedRoot) {
            upgradeTree(state.initializedRoot, state);
        }

        const pending = state.pending.get(normalized);
        if (pending) {
            pending.resolve(constructor);
            state.pending.delete(normalized);
        }
    }, 2);

    method('get', function get(name) {
        requireArgument('get', arguments, 1);
        const state = requireRegistry(this);
        return state.definitions.get(String(name))?.constructor;
    }, 1);

    method('getName', function getName(constructor) {
        requireArgument('getName', arguments, 1);
        return requireRegistry(this).constructors.get(constructor) || null;
    }, 1);

    method('upgrade', function upgrade(root) {
        requireArgument('upgrade', arguments, 1);
        upgradeTree(root, requireRegistry(this));
    }, 1);

    method('whenDefined', function whenDefined(name) {
        requireArgument('whenDefined', arguments, 1);
        const state = requireRegistry(this);
        const normalized = String(name);
        if (!isValidName(normalized)) return Promise.reject(syntaxError('whenDefined', normalized));
        const existing = state.definitions.get(normalized);
        if (existing) return Promise.resolve(existing.constructor);
        let pending = state.pending.get(normalized);
        if (!pending) {
            let resolve;
            const promise = new Promise(r => { resolve = r; });
            pending = { promise, resolve };
            state.pending.set(normalized, pending);
        }
        return pending.promise;
    }, 1);

    method('initialize', function initialize(root) {
        requireArgument('initialize', arguments, 1);
        const state = requireRegistry(this);
        state.initializedRoot = root;
        upgradeTree(root, state);
    }, 1);

    Object.defineProperty(proto, 'constructor', {
        value: CustomElementRegistry, writable: true, enumerable: false, configurable: true,
    });
    Object.defineProperty(proto, Symbol.toStringTag, {
        value: 'CustomElementRegistry', enumerable: false, configurable: true,
    });

    globalThis.CustomElementRegistry = CustomElementRegistry;
    const globalRegistry = new CustomElementRegistry();
    globalThis.customElements = globalRegistry;
    const globalState = requireRegistry(globalRegistry);

    let domBridge = null;
    const hooks = {
        created(element, tag, options, documentObject) {
            const lowerTag = String(tag).toLowerCase();
            const isName = options && typeof options === 'object' && options.is != null
                ? String(options.is).toLowerCase() : null;
            let entry = null;
            if (isName) {
                const candidate = globalState.definitions.get(isName);
                if (candidate && candidate.extends === lowerTag) entry = candidate;
            } else {
                const candidate = globalState.definitions.get(lowerTag);
                if (candidate && !candidate.extends) entry = candidate;
            }
            return entry ? upgradeElement(element, entry) : element;
        },
        connected(element) {
            if (upgraded.has(element)) callLifecycle(element, 'connectedCallback');
        },
        disconnected(element) {
            if (upgraded.has(element)) callLifecycle(element, 'disconnectedCallback');
        },
        attributeChanged(element, name, oldValue, newValue) {
            if (!upgraded.has(element)) return;
            const entry = definitionForElement(element);
            if (!entry || !entry.observedAttributes.includes(String(name).toLowerCase())) return;
            callLifecycle(element, 'attributeChangedCallback', [String(name), oldValue, newValue]);
        },
    };
    domBridge = installDomHooks(hooks);

    construction.setNewFactory((newTarget, baseName) => {
        const name = globalState.constructors.get(newTarget);
        if (!name || !domBridge || typeof domBridge.createElement !== 'function') return null;
        const entry = globalState.definitions.get(name);
        if (!entry) return null;
        const expectedBase = entry.extends
            ? String(domBridge.createElement(globalThis.document, entry.extends).constructor?.name || '')
            : 'HTMLElement';
        if (expectedBase && expectedBase !== baseName) return null;
        const element = domBridge.createElement(globalThis.document, entry.extends || entry.name);
        Object.setPrototypeOf(element, newTarget.prototype);
        elementDefinitions.set(element, entry);
        upgraded.add(element);
        return element;
    });

    // Replace the warm-reuse reaper so pooled pages clear the final registry,
    // not the obsolete bootstrap registry it replaced.
    Object.defineProperty(globalThis, '__resetCustomElements', {
        value: function __resetCustomElements() {
            globalState.definitions.clear();
            globalState.constructors.clear();
            globalState.pending.clear();
            globalState.initializedRoot = null;
            if (ops && typeof ops.op_dom_clear_custom_element_definitions === 'function') {
                try { ops.op_dom_clear_custom_element_definitions(); } catch (_) {}
            }
        },
        writable: true,
        configurable: true,
        enumerable: false,
    });

    // Preserve the legacy temporary alias for bootstrap code that looks it up
    // before cleanup; it points at the final definition map now.
    globalThis._customElementsRegistry = globalState.definitions;

    try { delete globalThis.__oxideCustomElementConstruction; } catch (_) {}
    try { delete globalThis.__oxideInstallCustomElementHooks; } catch (_) {}
})(globalThis);
