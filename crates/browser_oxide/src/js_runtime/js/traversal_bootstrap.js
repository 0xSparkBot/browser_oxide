// DOM Traversal APIs: NodeFilter, TreeWalker and NodeIterator.
// These are page-only WebIDL interfaces; workers do not expose them.
((globalThis) => {
    const FILTER_ACCEPT = 1;
    const FILTER_REJECT = 2;
    const FILTER_SKIP = 3;
    const SHOW_ALL = 0xFFFFFFFF;

    const NodeFilter = ({
        NodeFilter() { throw new TypeError('Illegal constructor'); }
    }).NodeFilter;
    const constants = {
        FILTER_ACCEPT, FILTER_REJECT, FILTER_SKIP,
        SHOW_ALL,
        SHOW_ELEMENT: 0x1,
        SHOW_ATTRIBUTE: 0x2,
        SHOW_TEXT: 0x4,
        SHOW_CDATA_SECTION: 0x8,
        SHOW_ENTITY_REFERENCE: 0x10,
        SHOW_ENTITY: 0x20,
        SHOW_PROCESSING_INSTRUCTION: 0x40,
        SHOW_COMMENT: 0x80,
        SHOW_DOCUMENT: 0x100,
        SHOW_DOCUMENT_TYPE: 0x200,
        SHOW_DOCUMENT_FRAGMENT: 0x400,
        SHOW_NOTATION: 0x800,
    };
    for (const [name, value] of Object.entries(constants)) {
        Object.defineProperty(NodeFilter, name, {
            value, writable: false, enumerable: true, configurable: false,
        });
    }

    const treeWalkerState = new WeakMap();
    const nodeIteratorState = new WeakMap();
    const requireState = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const children = node => {
        const list = node?.childNodes;
        if (!list) return [];
        const result = [];
        for (let index = 0; index < Number(list.length || 0); index++) {
            const child = list[index] ?? list.item?.(index);
            if (child) result.push(child);
        }
        return result;
    };
    const showBit = node => {
        const type = Number(node?.nodeType || 0);
        return type >= 1 && type <= 32 ? (2 ** (type - 1)) >>> 0 : 0;
    };
    const filterResult = (state, node) => {
        if (state.whatToShow !== SHOW_ALL && ((state.whatToShow >>> 0) & showBit(node)) === 0) {
            return FILTER_SKIP;
        }
        const filter = state.filter;
        if (filter == null) return FILTER_ACCEPT;
        let result;
        if (typeof filter === 'function') result = filter(node);
        else if (typeof filter.acceptNode === 'function') result = filter.acceptNode(node);
        else return FILTER_ACCEPT;
        result = Number(result);
        return result === FILTER_REJECT || result === FILTER_SKIP ? result : FILTER_ACCEPT;
    };
    const nextRaw = (node, root, skipChildren = false) => {
        if (!skipChildren) {
            const first = children(node)[0];
            if (first) return first;
        }
        let cursor = node;
        while (cursor && cursor !== root) {
            const parent = cursor.parentNode;
            if (!parent) return null;
            const siblings = children(parent);
            const index = siblings.indexOf(cursor);
            if (index >= 0 && index + 1 < siblings.length) return siblings[index + 1];
            cursor = parent;
        }
        return null;
    };
    const previousRaw = (node, root) => {
        if (!node || node === root) return null;
        const parent = node.parentNode;
        if (!parent) return null;
        const siblings = children(parent);
        const index = siblings.indexOf(node);
        if (index > 0) {
            let cursor = siblings[index - 1];
            while (true) {
                const descendants = children(cursor);
                if (!descendants.length) return cursor;
                cursor = descendants[descendants.length - 1];
            }
        }
        return parent;
    };
    const firstAcceptedInBranch = (state, node, reverse = false) => {
        const decision = filterResult(state, node);
        if (decision === FILTER_ACCEPT) return node;
        if (decision === FILTER_REJECT) return null;
        const list = children(node);
        if (reverse) list.reverse();
        for (const child of list) {
            const found = firstAcceptedInBranch(state, child, reverse);
            if (found) return found;
        }
        return null;
    };

    function TreeWalker() {
        throw new TypeError("Failed to construct 'TreeWalker': Illegal constructor");
    }
    Object.defineProperties(TreeWalker.prototype, {
        root: { get: function root() { return requireState(treeWalkerState, this).root; }, enumerable: true, configurable: true },
        whatToShow: { get: function whatToShow() { return requireState(treeWalkerState, this).whatToShow; }, enumerable: true, configurable: true },
        filter: { get: function filter() { return requireState(treeWalkerState, this).filter; }, enumerable: true, configurable: true },
        currentNode: {
            get: function currentNode() { return requireState(treeWalkerState, this).currentNode; },
            set: function currentNode(value) {
                if (!value || typeof value !== 'object' || typeof value.nodeType !== 'number') {
                    throw new TypeError("Failed to set the 'currentNode' property on 'TreeWalker': parameter 1 is not of type 'Node'.");
                }
                requireState(treeWalkerState, this).currentNode = value;
            },
            enumerable: true, configurable: true,
        },
    });
    const treeMethods = {
        parentNode() {
            const state = requireState(treeWalkerState, this);
            if (state.currentNode === state.root) return null;
            let cursor = state.currentNode.parentNode;
            while (cursor) {
                const decision = filterResult(state, cursor);
                if (decision === FILTER_ACCEPT) {
                    state.currentNode = cursor;
                    return cursor;
                }
                if (cursor === state.root) break;
                cursor = cursor.parentNode;
            }
            return null;
        },
        firstChild() {
            const state = requireState(treeWalkerState, this);
            for (const child of children(state.currentNode)) {
                const found = firstAcceptedInBranch(state, child, false);
                if (found) { state.currentNode = found; return found; }
            }
            return null;
        },
        lastChild() {
            const state = requireState(treeWalkerState, this);
            const list = children(state.currentNode).reverse();
            for (const child of list) {
                const found = firstAcceptedInBranch(state, child, true);
                if (found) { state.currentNode = found; return found; }
            }
            return null;
        },
        previousSibling() {
            const state = requireState(treeWalkerState, this);
            const parent = state.currentNode?.parentNode;
            if (!parent) return null;
            const siblings = children(parent);
            let index = siblings.indexOf(state.currentNode) - 1;
            for (; index >= 0; index--) {
                const found = firstAcceptedInBranch(state, siblings[index], true);
                if (found) { state.currentNode = found; return found; }
            }
            return null;
        },
        nextSibling() {
            const state = requireState(treeWalkerState, this);
            const parent = state.currentNode?.parentNode;
            if (!parent) return null;
            const siblings = children(parent);
            let index = siblings.indexOf(state.currentNode) + 1;
            for (; index < siblings.length; index++) {
                const found = firstAcceptedInBranch(state, siblings[index], false);
                if (found) { state.currentNode = found; return found; }
            }
            return null;
        },
        previousNode() {
            const state = requireState(treeWalkerState, this);
            let candidate = previousRaw(state.currentNode, state.root);
            while (candidate) {
                // A node beneath a rejected ancestor is not in the TreeWalker view.
                let ancestor = candidate.parentNode;
                let hidden = false;
                while (ancestor && ancestor !== state.root) {
                    if (filterResult(state, ancestor) === FILTER_REJECT) { hidden = true; break; }
                    ancestor = ancestor.parentNode;
                }
                if (!hidden && filterResult(state, candidate) === FILTER_ACCEPT) {
                    state.currentNode = candidate;
                    return candidate;
                }
                candidate = previousRaw(candidate, state.root);
            }
            return null;
        },
        nextNode() {
            const state = requireState(treeWalkerState, this);
            let candidate = nextRaw(state.currentNode, state.root, false);
            while (candidate) {
                const decision = filterResult(state, candidate);
                if (decision === FILTER_ACCEPT) {
                    state.currentNode = candidate;
                    return candidate;
                }
                candidate = nextRaw(candidate, state.root, decision === FILTER_REJECT);
            }
            return null;
        },
    };
    for (const [name, method] of Object.entries(treeMethods)) {
        Object.defineProperty(TreeWalker.prototype, name, {
            value: method, writable: true, enumerable: true, configurable: true,
        });
    }
    Object.defineProperty(TreeWalker.prototype, Symbol.toStringTag, { value: 'TreeWalker', configurable: true });

    function NodeIterator() {
        throw new TypeError("Failed to construct 'NodeIterator': Illegal constructor");
    }
    Object.defineProperties(NodeIterator.prototype, {
        root: { get: function root() { return requireState(nodeIteratorState, this).root; }, enumerable: true, configurable: true },
        referenceNode: { get: function referenceNode() { return requireState(nodeIteratorState, this).referenceNode; }, enumerable: true, configurable: true },
        pointerBeforeReferenceNode: { get: function pointerBeforeReferenceNode() { return requireState(nodeIteratorState, this).pointerBeforeReferenceNode; }, enumerable: true, configurable: true },
        whatToShow: { get: function whatToShow() { return requireState(nodeIteratorState, this).whatToShow; }, enumerable: true, configurable: true },
        filter: { get: function filter() { return requireState(nodeIteratorState, this).filter; }, enumerable: true, configurable: true },
    });
    const iteratorMethods = {
        nextNode() {
            const state = requireState(nodeIteratorState, this);
            let candidate = state.pointerBeforeReferenceNode
                ? state.referenceNode
                : nextRaw(state.referenceNode, state.root, false);
            while (candidate) {
                if (filterResult(state, candidate) === FILTER_ACCEPT) {
                    state.referenceNode = candidate;
                    state.pointerBeforeReferenceNode = false;
                    return candidate;
                }
                candidate = nextRaw(candidate, state.root, false);
            }
            return null;
        },
        previousNode() {
            const state = requireState(nodeIteratorState, this);
            let candidate = state.pointerBeforeReferenceNode
                ? previousRaw(state.referenceNode, state.root)
                : state.referenceNode;
            while (candidate) {
                if (filterResult(state, candidate) === FILTER_ACCEPT) {
                    state.referenceNode = candidate;
                    state.pointerBeforeReferenceNode = true;
                    return candidate;
                }
                candidate = previousRaw(candidate, state.root);
            }
            return null;
        },
        detach() {},
    };
    for (const [name, method] of Object.entries(iteratorMethods)) {
        Object.defineProperty(NodeIterator.prototype, name, {
            value: method, writable: true, enumerable: true, configurable: true,
        });
    }
    Object.defineProperty(NodeIterator.prototype, Symbol.toStringTag, { value: 'NodeIterator', configurable: true });

    const makeTreeWalker = (root, whatToShow = SHOW_ALL, filter = null) => {
        const walker = Object.create(TreeWalker.prototype);
        treeWalkerState.set(walker, {
            root,
            whatToShow: Number(whatToShow) >>> 0,
            filter: filter == null ? null : filter,
            currentNode: root,
        });
        return walker;
    };
    const makeNodeIterator = (root, whatToShow = SHOW_ALL, filter = null) => {
        const iterator = Object.create(NodeIterator.prototype);
        nodeIteratorState.set(iterator, {
            root,
            whatToShow: Number(whatToShow) >>> 0,
            filter: filter == null ? null : filter,
            referenceNode: root,
            pointerBeforeReferenceNode: true,
        });
        return iterator;
    };

    globalThis.NodeFilter = NodeFilter;
    globalThis.TreeWalker = TreeWalker;
    globalThis.NodeIterator = NodeIterator;

    const docProto = globalThis.Document?.prototype;
    if (docProto) {
        Object.defineProperty(docProto, 'createTreeWalker', {
            value: function createTreeWalker(root) {
                if (!root || typeof root !== 'object' || typeof root.nodeType !== 'number') {
                    throw new TypeError("Failed to execute 'createTreeWalker' on 'Document': parameter 1 is not of type 'Node'.");
                }
                return makeTreeWalker(root, arguments.length > 1 ? arguments[1] : SHOW_ALL, arguments.length > 2 ? arguments[2] : null);
            },
            writable: true, enumerable: true, configurable: true,
        });
        Object.defineProperty(docProto, 'createNodeIterator', {
            value: function createNodeIterator(root) {
                if (!root || typeof root !== 'object' || typeof root.nodeType !== 'number') {
                    throw new TypeError("Failed to execute 'createNodeIterator' on 'Document': parameter 1 is not of type 'Node'.");
                }
                return makeNodeIterator(root, arguments.length > 1 ? arguments[1] : SHOW_ALL, arguments.length > 2 ? arguments[2] : null);
            },
            writable: true, enumerable: true, configurable: true,
        });
    }

    try {
        if (typeof _maskFunction === 'function') {
            _maskFunction(NodeFilter, 'NodeFilter');
            _maskFunction(TreeWalker, 'TreeWalker');
            _maskFunction(NodeIterator, 'NodeIterator');
        }
        if (typeof _maskAsNative === 'function') {
            _maskAsNative(TreeWalker.prototype, ...Object.keys(treeMethods), 'root', 'whatToShow', 'filter', 'currentNode');
            _maskAsNative(NodeIterator.prototype, ...Object.keys(iteratorMethods), 'root', 'referenceNode', 'pointerBeforeReferenceNode', 'whatToShow', 'filter');
            if (docProto) _maskAsNative(docProto, 'createTreeWalker', 'createNodeIterator');
        }
    } catch (_) {}
})(globalThis);
