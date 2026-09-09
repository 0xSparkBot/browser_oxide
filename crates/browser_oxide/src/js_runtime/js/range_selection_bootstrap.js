// Range / Selection WebIDL implementation built on BrowserOxide's public DOM
// surface. Boundary and selection state live in WeakMaps so instances expose
// Chromium's empty own-property shape.
((globalThis) => {
    if (!globalThis.document || !globalThis.Node) return;

    const rangeState = new WeakMap();
    const selectionState = new WeakMap();
    const rectListState = new WeakMap();

    const stateForRange = value => {
        const state = rangeState.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const stateForSelection = value => {
        const state = selectionState.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const children = node => {
        const list = node?.childNodes;
        if (!list) return [];
        const out = [];
        for (let index = 0; index < Number(list.length || 0); index++) {
            const child = list[index] ?? list.item?.(index);
            if (child) out.push(child);
        }
        return out;
    };
    const nodeLength = node => {
        const type = Number(node?.nodeType || 0);
        if (type === 3 || type === 4 || type === 8) {
            return String(node.data ?? node.nodeValue ?? '').length;
        }
        return children(node).length;
    };
    const validateNode = node => {
        if (!node || typeof node !== 'object' || typeof node.nodeType !== 'number') {
            throw new TypeError("parameter 1 is not of type 'Node'.");
        }
    };
    const validateOffset = (node, offset) => {
        const numeric = Number(offset) >>> 0;
        if (numeric > nodeLength(node)) {
            try { throw new DOMException('The offset is larger than the node length.', 'IndexSizeError'); }
            catch (error) { throw error; }
        }
        return numeric;
    };
    const rootOf = node => {
        let cursor = node;
        while (cursor?.parentNode) cursor = cursor.parentNode;
        return cursor;
    };
    const pathFromRoot = node => {
        const path = [];
        let cursor = node;
        while (cursor) { path.unshift(cursor); cursor = cursor.parentNode; }
        return path;
    };
    const siblingIndex = node => {
        const parent = node?.parentNode;
        if (!parent) return -1;
        return children(parent).indexOf(node);
    };
    // -1 => a before b, 0 => equal, 1 => a after b.
    const comparePoints = (aNode, aOffset, bNode, bOffset) => {
        if (aNode === bNode) return aOffset < bOffset ? -1 : aOffset > bOffset ? 1 : 0;
        if (rootOf(aNode) !== rootOf(bNode)) {
            try { throw new DOMException('The two boundary points are in different trees.', 'WrongDocumentError'); }
            catch (error) { throw error; }
        }
        const aPath = pathFromRoot(aNode);
        const bPath = pathFromRoot(bNode);
        let common = 0;
        while (common < aPath.length && common < bPath.length && aPath[common] === bPath[common]) common++;
        if (common === aPath.length) {
            // aNode is ancestor of bNode. The child directly below aNode
            // determines which side of aOffset contains the b boundary.
            const child = bPath[common];
            const index = children(aNode).indexOf(child);
            return aOffset <= index ? -1 : 1;
        }
        if (common === bPath.length) {
            const child = aPath[common];
            const index = children(bNode).indexOf(child);
            return index < bOffset ? -1 : 1;
        }
        const parent = aPath[common - 1];
        const siblings = children(parent);
        return siblings.indexOf(aPath[common]) < siblings.indexOf(bPath[common]) ? -1 : 1;
    };
    const commonAncestor = (a, b) => {
        const aPath = pathFromRoot(a), bPath = pathFromRoot(b);
        let last = null;
        for (let i = 0; i < Math.min(aPath.length, bPath.length); i++) {
            if (aPath[i] !== bPath[i]) break;
            last = aPath[i];
        }
        return last;
    };
    const updateCollapsed = state => {
        state.collapsed = state.startContainer === state.endContainer
            && state.startOffset === state.endOffset;
    };
    const setStartBoundary = (state, node, offset) => {
        validateNode(node); offset = validateOffset(node, offset);
        if (rootOf(node) !== rootOf(state.endContainer)
            || comparePoints(node, offset, state.endContainer, state.endOffset) > 0) {
            state.endContainer = node;
            state.endOffset = offset;
        }
        state.startContainer = node;
        state.startOffset = offset;
        updateCollapsed(state);
    };
    const setEndBoundary = (state, node, offset) => {
        validateNode(node); offset = validateOffset(node, offset);
        if (rootOf(node) !== rootOf(state.startContainer)
            || comparePoints(node, offset, state.startContainer, state.startOffset) < 0) {
            state.startContainer = node;
            state.startOffset = offset;
        }
        state.endContainer = node;
        state.endOffset = offset;
        updateCollapsed(state);
    };
    const boundaryOfNode = node => {
        const parent = node?.parentNode;
        if (!parent) return null;
        const index = siblingIndex(node);
        return { parent, start:index, end:index + 1 };
    };
    const intersectsRange = (state, node) => {
        const boundary = boundaryOfNode(node);
        if (!boundary) return node === state.startContainer || node === state.endContainer;
        return comparePoints(boundary.parent, boundary.end, state.startContainer, state.startOffset) > 0
            && comparePoints(boundary.parent, boundary.start, state.endContainer, state.endOffset) < 0;
    };
    const fullyContained = (state, node) => {
        const boundary = boundaryOfNode(node);
        if (!boundary) return false;
        return comparePoints(state.startContainer, state.startOffset, boundary.parent, boundary.start) <= 0
            && comparePoints(boundary.parent, boundary.end, state.endContainer, state.endOffset) <= 0;
    };
    const textData = node => String(node?.data ?? node?.nodeValue ?? node?.textContent ?? '');
    const setTextData = (node, value) => {
        try { node.data = String(value); return; } catch (_) {}
        try { node.nodeValue = String(value); return; } catch (_) {}
        try { node.textContent = String(value); } catch (_) {}
    };

    // --- DOMRectList -------------------------------------------------------
    function DOMRectList() {
        throw new TypeError("Failed to construct 'DOMRectList': Illegal constructor");
    }
    Object.defineProperty(DOMRectList.prototype, 'length', {
        get:function length() { return rectListState.get(this)?.length || 0; },
        enumerable:true, configurable:true,
    });
    Object.defineProperty(DOMRectList.prototype, 'item', {
        value:function item(index) {
            const list = rectListState.get(this) || [];
            return list[Number(index) >>> 0] ?? null;
        },
        writable:true, enumerable:true, configurable:true,
    });
    Object.defineProperty(DOMRectList.prototype, Symbol.toStringTag, {
        value:'DOMRectList', configurable:true,
    });
    const rectValues = function values() {
        const list = rectListState.get(this) || [];
        return list[Symbol.iterator]();
    };
    Object.defineProperty(DOMRectList.prototype, Symbol.iterator, {
        value:rectValues, writable:true, configurable:true,
    });
    const makeRectList = rects => {
        const target = Object.create(DOMRectList.prototype);
        const list = Array.from(rects || []);
        const proxy = new Proxy(target, {
            get(object, prop, receiver) {
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    return list[Number(prop)];
                }
                return Reflect.get(object, prop, receiver);
            },
            has(object, prop) {
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) return Number(prop) < list.length;
                return Reflect.has(object, prop);
            },
            ownKeys() { return list.map((_, index) => String(index)); },
            getOwnPropertyDescriptor(object, prop) {
                if (typeof prop === 'string' && /^(0|[1-9]\d*)$/.test(prop)) {
                    const index = Number(prop);
                    if (index >= list.length) return undefined;
                    return { value:list[index], writable:false, enumerable:true, configurable:true };
                }
                return Reflect.getOwnPropertyDescriptor(object, prop);
            },
        });
        rectListState.set(target, list);
        rectListState.set(proxy, list);
        return proxy;
    };

    // --- Range contents helpers -------------------------------------------
    const textSliceForRange = (state, node) => {
        if (!intersectsRange(state, node) && node !== state.startContainer && node !== state.endContainer) return null;
        const data = textData(node);
        let start = 0, end = data.length;
        if (node === state.startContainer) start = state.startOffset;
        if (node === state.endContainer) end = state.endOffset;
        if (end < start) return '';
        return data.slice(start, end);
    };
    const selectedClone = (state, node) => {
        const type = Number(node?.nodeType || 0);
        if (type === 3 || type === 4) {
            const selected = textSliceForRange(state, node);
            return selected == null ? null : globalThis.document.createTextNode(selected);
        }
        if (fullyContained(state, node)) {
            try { return node.cloneNode(true); } catch (_) {}
        }
        if (!intersectsRange(state, node)
            && node !== state.startContainer && node !== state.endContainer
            && !pathFromRoot(state.startContainer).includes(node)
            && !pathFromRoot(state.endContainer).includes(node)) return null;
        let clone = null;
        try { clone = node.cloneNode(false); } catch (_) {
            if (type === 11) clone = globalThis.document.createDocumentFragment();
        }
        if (!clone) return null;
        let appended = false;
        for (const child of children(node)) {
            const childClone = selectedClone(state, child);
            if (childClone) { clone.appendChild(childClone); appended = true; }
        }
        // Keep empty fully-selected elements.
        return appended ? clone : (fullyContained(state, node) ? clone : null);
    };
    const cloneContentsImpl = state => {
        const fragment = globalThis.document.createDocumentFragment();
        if (state.collapsed) return fragment;
        if (state.startContainer === state.endContainer
            && [3,4].includes(Number(state.startContainer.nodeType))) {
            fragment.appendChild(globalThis.document.createTextNode(
                textData(state.startContainer).slice(state.startOffset, state.endOffset)
            ));
            return fragment;
        }
        const common = commonAncestor(state.startContainer, state.endContainer);
        if (!common) return fragment;
        if ([3,4].includes(Number(common.nodeType))) {
            const selected = textSliceForRange(state, common);
            if (selected != null) fragment.appendChild(globalThis.document.createTextNode(selected));
            return fragment;
        }
        for (const child of children(common)) {
            const clone = selectedClone(state, child);
            if (clone) fragment.appendChild(clone);
        }
        return fragment;
    };
    const deleteSelected = (state, node, isCommon = false) => {
        const type = Number(node?.nodeType || 0);
        if (type === 3 || type === 4) {
            const data = textData(node);
            if (node === state.startContainer && node === state.endContainer) {
                setTextData(node, data.slice(0,state.startOffset) + data.slice(state.endOffset));
                return;
            }
            if (node === state.startContainer) {
                setTextData(node, data.slice(0,state.startOffset));
                return;
            }
            if (node === state.endContainer) {
                setTextData(node, data.slice(state.endOffset));
                return;
            }
            if (fullyContained(state,node) && node.parentNode) {
                node.parentNode.removeChild(node);
            }
            return;
        }
        for (const child of children(node).slice()) {
            if (fullyContained(state, child) && child !== state.startContainer && child !== state.endContainer) {
                try { node.removeChild(child); } catch (_) { deleteSelected(state, child); }
            } else if (intersectsRange(state, child)
                || pathFromRoot(state.startContainer).includes(child)
                || pathFromRoot(state.endContainer).includes(child)) {
                deleteSelected(state, child, false);
            }
        }
    };
    const firstElementForRect = state => {
        let node = state.startContainer;
        if (Number(node?.nodeType) === 3) node = node.parentElement || node.parentNode;
        return node && typeof node.getBoundingClientRect === 'function' ? node : null;
    };

    // --- AbstractRange / Range --------------------------------------------
    function AbstractRange() {
        throw new TypeError("Failed to construct 'AbstractRange': Illegal constructor");
    }
    const abstractRangeGetters = {
        startContainer:s=>s.startContainer, startOffset:s=>s.startOffset,
        endContainer:s=>s.endContainer, endOffset:s=>s.endOffset,
        collapsed:s=>s.collapsed,
    };
    for (const [name,getValue] of Object.entries(abstractRangeGetters)) {
        Object.defineProperty(AbstractRange.prototype,name,{
            get:function(){return getValue(stateForRange(this));}, enumerable:true, configurable:true,
        });
    }
    Object.defineProperty(AbstractRange.prototype,Symbol.toStringTag,{
        value:'AbstractRange', configurable:true,
    });

    function Range() {
        if (!new.target) {
            throw new TypeError("Failed to construct 'Range': Please use the 'new' operator, this DOM object constructor cannot be called as a function.");
        }
        rangeState.set(this, {
            startContainer:globalThis.document, startOffset:0,
            endContainer:globalThis.document, endOffset:0, collapsed:true,
        });
    }
    Range.prototype = Object.create(AbstractRange.prototype);
    Object.defineProperty(Range.prototype,'constructor',{
        value:Range, writable:true, configurable:true,
    });
    const rangeGetters = {
        commonAncestorContainer:s=>commonAncestor(s.startContainer,s.endContainer),
    };
    for (const [name,getValue] of Object.entries(rangeGetters)) {
        Object.defineProperty(Range.prototype,name,{
            get:function(){return getValue(stateForRange(this));}, enumerable:true, configurable:true,
        });
    }
    const rangeMethods = {
        setStart(node, offset) { setStartBoundary(stateForRange(this),node,offset); },
        setEnd(node, offset) { setEndBoundary(stateForRange(this),node,offset); },
        setStartBefore(node) { validateNode(node); const p=node.parentNode;if(!p)throw new DOMException('Invalid node.','InvalidNodeTypeError');setStartBoundary(stateForRange(this),p,siblingIndex(node)); },
        setStartAfter(node) { validateNode(node); const p=node.parentNode;if(!p)throw new DOMException('Invalid node.','InvalidNodeTypeError');setStartBoundary(stateForRange(this),p,siblingIndex(node)+1); },
        setEndBefore(node) { validateNode(node); const p=node.parentNode;if(!p)throw new DOMException('Invalid node.','InvalidNodeTypeError');setEndBoundary(stateForRange(this),p,siblingIndex(node)); },
        setEndAfter(node) { validateNode(node); const p=node.parentNode;if(!p)throw new DOMException('Invalid node.','InvalidNodeTypeError');setEndBoundary(stateForRange(this),p,siblingIndex(node)+1); },
        collapse() {
            const state=stateForRange(this); const toStart=arguments.length===0?false:!!arguments[0];
            if(toStart){state.endContainer=state.startContainer;state.endOffset=state.startOffset;}
            else{state.startContainer=state.endContainer;state.startOffset=state.endOffset;}
            state.collapsed=true;
        },
        selectNode(node) {
            validateNode(node);const p=node.parentNode;if(!p)throw new DOMException('Invalid node.','InvalidNodeTypeError');const i=siblingIndex(node);
            const s=stateForRange(this);s.startContainer=p;s.startOffset=i;s.endContainer=p;s.endOffset=i+1;s.collapsed=false;
        },
        selectNodeContents(node) {
            validateNode(node);const s=stateForRange(this);s.startContainer=node;s.startOffset=0;s.endContainer=node;s.endOffset=nodeLength(node);updateCollapsed(s);
        },
        compareBoundaryPoints(how, sourceRange) {
            const s=stateForRange(this), o=stateForRange(sourceRange);let aNode,aOffset,bNode,bOffset;
            switch(Number(how)){
                case 0:aNode=s.startContainer;aOffset=s.startOffset;bNode=o.startContainer;bOffset=o.startOffset;break;
                case 1:aNode=s.endContainer;aOffset=s.endOffset;bNode=o.startContainer;bOffset=o.startOffset;break;
                case 2:aNode=s.endContainer;aOffset=s.endOffset;bNode=o.endContainer;bOffset=o.endOffset;break;
                case 3:aNode=s.startContainer;aOffset=s.startOffset;bNode=o.endContainer;bOffset=o.endOffset;break;
                default:throw new DOMException('Invalid comparison mode.','NotSupportedError');
            }
            return comparePoints(aNode,aOffset,bNode,bOffset);
        },
        deleteContents() {
            const s=stateForRange(this);if(s.collapsed)return;const startNode=s.startContainer,startOffset=s.startOffset;
            if(startNode===s.endContainer&&[3,4].includes(Number(startNode.nodeType))){deleteSelected(s,startNode,true);s.endContainer=startNode;s.endOffset=startOffset;s.collapsed=true;return;}
            const common=commonAncestor(s.startContainer,s.endContainer);if(common)deleteSelected(s,common,true);
            s.startContainer=startNode;s.startOffset=Math.min(startOffset,nodeLength(startNode));s.endContainer=s.startContainer;s.endOffset=s.startOffset;s.collapsed=true;
        },
        extractContents() { const fragment=cloneContentsImpl(stateForRange(this));this.deleteContents();return fragment; },
        cloneContents() { return cloneContentsImpl(stateForRange(this)); },
        insertNode(newNode) {
            validateNode(newNode);const s=stateForRange(this);const container=s.startContainer,offset=s.startOffset;
            if([3,4].includes(Number(container.nodeType))){
                const parent=container.parentNode;if(!parent)throw new DOMException('No parent.','HierarchyRequestError');
                const data=textData(container),before=data.slice(0,offset),after=data.slice(offset);const ref=container.nextSibling;
                setTextData(container,before);parent.insertBefore(newNode,ref);if(after)parent.insertBefore(globalThis.document.createTextNode(after),ref);
            }else{
                const list=children(container);container.insertBefore(newNode,list[offset]||null);
            }
        },
        surroundContents(newParent) { validateNode(newParent);const fragment=this.extractContents();while(newParent.firstChild)newParent.removeChild(newParent.firstChild);this.insertNode(newParent);newParent.appendChild(fragment);this.selectNode(newParent); },
        cloneRange() {
            const s=stateForRange(this),r=new Range(),o=stateForRange(r);Object.assign(o,s);return r;
        },
        detach() {},
        isPointInRange(node, offset) { validateNode(node);offset=validateOffset(node,offset);const s=stateForRange(this);if(rootOf(node)!==rootOf(s.startContainer))return false;return comparePoints(s.startContainer,s.startOffset,node,offset)<=0&&comparePoints(node,offset,s.endContainer,s.endOffset)<=0; },
        comparePoint(node, offset) { validateNode(node);offset=validateOffset(node,offset);const s=stateForRange(this);if(rootOf(node)!==rootOf(s.startContainer))throw new DOMException('Different tree.','WrongDocumentError');if(comparePoints(node,offset,s.startContainer,s.startOffset)<0)return -1;if(comparePoints(node,offset,s.endContainer,s.endOffset)>0)return 1;return 0; },
        intersectsNode(node) { validateNode(node);return intersectsRange(stateForRange(this),node); },
        getClientRects() {
            const s=stateForRange(this);if(s.collapsed)return makeRectList([]);const element=firstElementForRect(s);let rect=null;try{rect=element?.getBoundingClientRect?.()||null;}catch(_){}return makeRectList(rect?[rect]:[]);
        },
        getBoundingClientRect() {
            const list=this.getClientRects();if(!list.length)return new globalThis.DOMRect();
            let left=Infinity,top=Infinity,right=-Infinity,bottom=-Infinity;for(const rect of list){left=Math.min(left,rect.left);top=Math.min(top,rect.top);right=Math.max(right,rect.right);bottom=Math.max(bottom,rect.bottom);}return new globalThis.DOMRect(left,top,right-left,bottom-top);
        },
        createContextualFragment(fragment) {
            const div=globalThis.document.createElement('div');div.innerHTML=String(fragment);const out=globalThis.document.createDocumentFragment();while(div.firstChild)out.appendChild(div.firstChild);return out;
        },
        expand() {},
        toString() { const fragment=cloneContentsImpl(stateForRange(this));return String(fragment.textContent||''); },
    };
    const rangeLengths={setStart:2,setEnd:2,setStartBefore:1,setStartAfter:1,setEndBefore:1,setEndAfter:1,collapse:0,selectNode:1,selectNodeContents:1,compareBoundaryPoints:2,deleteContents:0,extractContents:0,cloneContents:0,insertNode:1,surroundContents:1,cloneRange:0,detach:0,isPointInRange:2,comparePoint:2,intersectsNode:1,getClientRects:0,getBoundingClientRect:0,createContextualFragment:1,expand:0,toString:0};
    for(const [name,method] of Object.entries(rangeMethods)){Object.defineProperty(method,'length',{value:rangeLengths[name],configurable:true});Object.defineProperty(Range.prototype,name,{value:method,writable:true,enumerable:true,configurable:true});}
    for(const [name,value] of Object.entries({START_TO_START:0,START_TO_END:1,END_TO_END:2,END_TO_START:3})){
        Object.defineProperty(Range,name,{value,writable:false,enumerable:true,configurable:false});
        Object.defineProperty(Range.prototype,name,{value,writable:false,enumerable:true,configurable:false});
    }
    Object.defineProperty(Range.prototype,Symbol.toStringTag,{value:'Range',configurable:true});

    // --- Selection ---------------------------------------------------------
    function Selection() { throw new TypeError("Failed to construct 'Selection': Illegal constructor"); }
    const makeSelection = () => {
        const selection=Object.create(Selection.prototype);selectionState.set(selection,{ranges:[],anchorNode:null,anchorOffset:0,focusNode:null,focusOffset:0,direction:'none'});return selection;
    };
    const selectionGetters={
        anchorNode:s=>s.anchorNode,anchorOffset:s=>s.anchorOffset,
        focusNode:s=>s.focusNode,focusOffset:s=>s.focusOffset,
        baseNode:s=>s.anchorNode,baseOffset:s=>s.anchorOffset,
        extentNode:s=>s.focusNode,extentOffset:s=>s.focusOffset,
        isCollapsed:s=>s.anchorNode===s.focusNode&&s.anchorOffset===s.focusOffset,
        rangeCount:s=>s.ranges.length,direction:s=>s.direction,
        type:s=>s.ranges.length===0?'None':(s.anchorNode===s.focusNode&&s.anchorOffset===s.focusOffset?'Caret':'Range'),
    };
    for(const [name,getValue] of Object.entries(selectionGetters)){Object.defineProperty(Selection.prototype,name,{get:function(){return getValue(stateForSelection(this));},enumerable:true,configurable:true});}
    const collapseSelection=(selection,node,offset=0)=>{validateNode(node);offset=validateOffset(node,offset);const s=stateForSelection(selection),r=new Range(),rs=stateForRange(r);rs.startContainer=node;rs.startOffset=offset;rs.endContainer=node;rs.endOffset=offset;rs.collapsed=true;s.ranges=[r];s.anchorNode=node;s.anchorOffset=offset;s.focusNode=node;s.focusOffset=offset;s.direction='none';};
    const selectionMethods={
        getRangeAt(index){const s=stateForSelection(this),i=Number(index)>>>0;if(i>=s.ranges.length)throw new DOMException('Invalid range index.','IndexSizeError');return s.ranges[i];},
        addRange(range){stateForRange(range);const s=stateForSelection(this);if(s.ranges.length)return; s.ranges=[range];const r=stateForRange(range);s.anchorNode=r.startContainer;s.anchorOffset=r.startOffset;s.focusNode=r.endContainer;s.focusOffset=r.endOffset;s.direction='none';},
        removeRange(range){const s=stateForSelection(this);const i=s.ranges.indexOf(range);if(i<0)throw new DOMException('The given range is not in the selection.','NotFoundError');s.ranges.splice(i,1);if(!s.ranges.length){s.anchorNode=s.focusNode=null;s.anchorOffset=s.focusOffset=0;s.direction='none';}},
        removeAllRanges(){const s=stateForSelection(this);s.ranges=[];s.anchorNode=s.focusNode=null;s.anchorOffset=s.focusOffset=0;s.direction='none';},
        empty(){this.removeAllRanges();},
        collapse(node){if(node==null){this.removeAllRanges();return;}collapseSelection(this,node,arguments.length>1?arguments[1]:0);},
        setPosition(node){return this.collapse(node,arguments.length>1?arguments[1]:0);},
        collapseToStart(){const s=stateForSelection(this);if(!s.ranges.length)throw new DOMException('There is no selection.','InvalidStateError');const r=stateForRange(s.ranges[0]);collapseSelection(this,r.startContainer,r.startOffset);},
        collapseToEnd(){const s=stateForSelection(this);if(!s.ranges.length)throw new DOMException('There is no selection.','InvalidStateError');const r=stateForRange(s.ranges[0]);collapseSelection(this,r.endContainer,r.endOffset);},
        extend(node){validateNode(node);const offset=validateOffset(node,arguments.length>1?arguments[1]:0);const s=stateForSelection(this);if(!s.anchorNode)throw new DOMException('There is no selection.','InvalidStateError');const cmp=comparePoints(s.anchorNode,s.anchorOffset,node,offset);const r=new Range(),rs=stateForRange(r);if(cmp<=0){rs.startContainer=s.anchorNode;rs.startOffset=s.anchorOffset;rs.endContainer=node;rs.endOffset=offset;s.direction=cmp===0?'none':'forward';}else{rs.startContainer=node;rs.startOffset=offset;rs.endContainer=s.anchorNode;rs.endOffset=s.anchorOffset;s.direction='backward';}updateCollapsed(rs);s.ranges=[r];s.focusNode=node;s.focusOffset=offset;},
        setBaseAndExtent(anchorNode,anchorOffset,focusNode,focusOffset){validateNode(anchorNode);validateNode(focusNode);anchorOffset=validateOffset(anchorNode,anchorOffset);focusOffset=validateOffset(focusNode,focusOffset);const s=stateForSelection(this);s.anchorNode=anchorNode;s.anchorOffset=anchorOffset;s.focusNode=focusNode;s.focusOffset=focusOffset;const cmp=comparePoints(anchorNode,anchorOffset,focusNode,focusOffset);const r=new Range(),rs=stateForRange(r);if(cmp<=0){rs.startContainer=anchorNode;rs.startOffset=anchorOffset;rs.endContainer=focusNode;rs.endOffset=focusOffset;s.direction=cmp===0?'none':'forward';}else{rs.startContainer=focusNode;rs.startOffset=focusOffset;rs.endContainer=anchorNode;rs.endOffset=anchorOffset;s.direction='backward';}updateCollapsed(rs);s.ranges=[r];},
        selectAllChildren(node){validateNode(node);this.setBaseAndExtent(node,0,node,nodeLength(node));},
        deleteFromDocument(){const s=stateForSelection(this);if(!s.ranges.length)return;const r=s.ranges[0],rs=stateForRange(r),n=rs.startContainer,o=rs.startOffset;r.deleteContents();collapseSelection(this,n,Math.min(o,nodeLength(n)));},
        containsNode(node){validateNode(node);const allowPartial=arguments.length>1?!!arguments[1]:false;const s=stateForSelection(this);if(!s.ranges.length)return false;const r=stateForRange(s.ranges[0]),b=boundaryOfNode(node);if(!b)return false;const start=comparePoints(r.startContainer,r.startOffset,b.parent,b.start),end=comparePoints(b.parent,b.end,r.endContainer,r.endOffset);if(allowPartial)return intersectsRange(r,node);return start<=0&&end<=0;},
        modify() {},
        getComposedRanges(){const s=stateForSelection(this);return s.ranges.map(range=>{const r=stateForRange(range);if(typeof globalThis.StaticRange==='function'){try{return new globalThis.StaticRange({startContainer:r.startContainer,startOffset:r.startOffset,endContainer:r.endContainer,endOffset:r.endOffset});}catch(_){}}return {startContainer:r.startContainer,startOffset:r.startOffset,endContainer:r.endContainer,endOffset:r.endOffset};});},
        toString(){const s=stateForSelection(this);return s.ranges.map(range=>String(range)).join('');},
    };
    const selectionLengths={getRangeAt:1,addRange:1,removeRange:1,removeAllRanges:0,empty:0,collapse:1,setPosition:1,collapseToStart:0,collapseToEnd:0,extend:1,setBaseAndExtent:4,selectAllChildren:1,deleteFromDocument:0,containsNode:1,modify:0,getComposedRanges:0,toString:0};
    for(const [name,method] of Object.entries(selectionMethods)){Object.defineProperty(method,'length',{value:selectionLengths[name],configurable:true});Object.defineProperty(Selection.prototype,name,{value:method,writable:true,enumerable:true,configurable:true});}
    Object.defineProperty(Selection.prototype,Symbol.toStringTag,{value:'Selection',configurable:true});
    const selection=makeSelection();

    globalThis.DOMRectList=DOMRectList;
    globalThis.AbstractRange=AbstractRange;
    globalThis.Range=Range;
    globalThis.Selection=Selection;
    globalThis.getSelection=function getSelection(){return selection;};
    const docProto=globalThis.Document?.prototype;
    if(docProto){
        Object.defineProperty(docProto,'createRange',{value:function createRange(){return new Range();},writable:true,enumerable:true,configurable:true});
        Object.defineProperty(docProto,'getSelection',{value:function getSelection(){return selection;},writable:true,enumerable:true,configurable:true});
    }

    try{
        if(typeof _maskFunction==='function'){_maskFunction(DOMRectList,'DOMRectList');_maskFunction(rectValues,'values');_maskFunction(AbstractRange,'AbstractRange');_maskFunction(Range,'Range');_maskFunction(Selection,'Selection');_maskFunction(globalThis.getSelection,'getSelection');}
        if(typeof _maskAsNative==='function'){
            _maskAsNative(DOMRectList.prototype,'length','item');
            _maskAsNative(AbstractRange.prototype,...Object.keys(abstractRangeGetters));
            _maskAsNative(Range.prototype,...Object.keys(rangeGetters),...Object.keys(rangeMethods));
            _maskAsNative(Selection.prototype,...Object.keys(selectionGetters),...Object.keys(selectionMethods));
            if(docProto)_maskAsNative(docProto,'createRange','getSelection');
        }
    }catch(_){}
})(globalThis);
