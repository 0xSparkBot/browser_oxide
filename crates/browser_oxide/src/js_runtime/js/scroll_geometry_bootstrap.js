// CSSOM View scroll geometry normalization.
//
// The Rust layout engine reports document-space boxes. Browser-facing
// getBoundingClientRect(), however, is viewport-relative and therefore moves
// opposite window.scrollX/scrollY. Keep the layout backend untouched and
// normalize at the Web API boundary. This layer also implements the common
// scrollIntoView alignment modes on top of the existing window scroll state.
(function () {
    'use strict';

    const proto = globalThis.Element && globalThis.Element.prototype;
    if (!proto) return;

    const rawGetBoundingClientRect = proto.getBoundingClientRect;
    if (typeof rawGetBoundingClientRect !== 'function') return;

    function _number(value) {
        const n = Number(value);
        return Number.isFinite(n) ? n : 0;
    }

    function _viewFor(element) {
        try {
            const view = element && element.ownerDocument && element.ownerDocument.defaultView;
            if (view) return view;
        } catch (_) {}
        return globalThis;
    }

    function _setViewScroll(view, x, y) {
        if (view && typeof view.scrollTo === 'function') {
            view.scrollTo(x, y);
            return;
        }
        // Same-isolate child realms currently expose scroll state before the
        // full Window scrolling method surface is installed. Keep the state
        // coherent so Element scrolling remains realm-local.
        try { view.scrollX = x; } catch (_) {}
        try { view.pageXOffset = x; } catch (_) {}
        try { view.scrollY = y; } catch (_) {}
        try { view.pageYOffset = y; } catch (_) {}
    }

    function getBoundingClientRect() {
        const rect = rawGetBoundingClientRect.call(this);
        const view = _viewFor(this);
        const sx = _number(view.scrollX);
        const sy = _number(view.scrollY);
        const x = _number(rect && (rect.x !== undefined ? rect.x : rect.left)) - sx;
        const y = _number(rect && (rect.y !== undefined ? rect.y : rect.top)) - sy;
        const width = _number(rect && rect.width);
        const height = _number(rect && rect.height);
        const Rect = (view && view.DOMRect) || globalThis.DOMRect;
        return new Rect(x, y, width, height);
    }

    function _enum(value, allowed, label) {
        const text = String(value);
        if (!allowed.includes(text)) {
            throw new TypeError(`Failed to execute 'scrollIntoView' on 'Element': The provided value '${text}' is not a valid enum value of type ${label}.`);
        }
        return text;
    }

    function _align(documentStart, size, viewportSize, currentScroll, mode) {
        if (mode === 'start') return documentStart;
        if (mode === 'center') return documentStart - (viewportSize - size) / 2;
        if (mode === 'end') return documentStart - (viewportSize - size);

        // `nearest`: move the minimum distance needed to make the box visible.
        const viewportStart = documentStart - currentScroll;
        const viewportEnd = viewportStart + size;
        if (viewportStart < 0 && viewportEnd > viewportSize) return currentScroll;
        if (viewportStart < 0) return documentStart;
        if (viewportEnd > viewportSize) return documentStart - (viewportSize - size);
        return currentScroll;
    }

    function scrollIntoView() {
        const arg = arguments.length ? arguments[0] : undefined;
        let block = 'start';
        let inline = 'nearest';

        if (arg === false) {
            block = 'end';
        } else if (arg && typeof arg === 'object') {
            if (arg.behavior !== undefined) {
                _enum(arg.behavior, ['auto', 'smooth', 'instant'], 'ScrollBehavior');
            }
            if (arg.block !== undefined) {
                block = _enum(arg.block, ['start', 'center', 'end', 'nearest'], 'ScrollLogicalPosition');
            }
            if (arg.inline !== undefined) {
                inline = _enum(arg.inline, ['start', 'center', 'end', 'nearest'], 'ScrollLogicalPosition');
            }
        }

        const rect = this.getBoundingClientRect();
        const view = _viewFor(this);
        const currentX = _number(view.scrollX);
        const currentY = _number(view.scrollY);
        const viewportWidth = _number(view.innerWidth);
        const viewportHeight = _number(view.innerHeight);
        const documentLeft = rect.left + currentX;
        const documentTop = rect.top + currentY;

        const nextX = Math.max(0, _align(documentLeft, rect.width, viewportWidth, currentX, inline));
        const nextY = Math.max(0, _align(documentTop, rect.height, viewportHeight, currentY, block));
        _setViewScroll(view, nextX, nextY);
    }


    // Chromium legacy API. Unlike scrollIntoView(), this only scrolls when
    // some part of the target is outside the viewport. With the default
    // centerIfNeeded=true a target that is completely off-screen is centered;
    // a partially clipped target still uses the minimum nearest-edge motion.
    // Passing false always uses nearest-edge alignment. This matches Chrome
    // 148's Element.scrollIntoViewIfNeeded() behavior.
    function scrollIntoViewIfNeeded() {
        const centerIfNeeded = arguments.length === 0 ? true : Boolean(arguments[0]);
        const rect = this.getBoundingClientRect();
        const view = _viewFor(this);
        const currentX = _number(view.scrollX);
        const currentY = _number(view.scrollY);
        const viewportWidth = _number(view.innerWidth);
        const viewportHeight = _number(view.innerHeight);

        const fullyVisibleX = rect.left >= 0 && rect.right <= viewportWidth;
        const fullyVisibleY = rect.top >= 0 && rect.bottom <= viewportHeight;
        if (fullyVisibleX && fullyVisibleY) return;

        const documentLeft = rect.left + currentX;
        const documentTop = rect.top + currentY;
        const fullyOutsideX = rect.right <= 0 || rect.left >= viewportWidth;
        const fullyOutsideY = rect.bottom <= 0 || rect.top >= viewportHeight;

        const horizontalMode = centerIfNeeded && fullyOutsideX ? 'center' : 'nearest';
        const verticalMode = centerIfNeeded && fullyOutsideY ? 'center' : 'nearest';
        const nextX = Math.max(0, _align(documentLeft, rect.width, viewportWidth, currentX, horizontalMode));
        const nextY = Math.max(0, _align(documentTop, rect.height, viewportHeight, currentY, verticalMode));
        _setViewScroll(view, nextX, nextY);
    }

    Object.defineProperty(proto, 'getBoundingClientRect', {
        value: getBoundingClientRect,
        writable: true,
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(proto, 'scrollIntoView', {
        value: scrollIntoView,
        writable: true,
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(proto, 'scrollIntoViewIfNeeded', {
        value: scrollIntoViewIfNeeded,
        writable: true,
        enumerable: true,
        configurable: true,
    });

    if (typeof globalThis._maskFunction === 'function') {
        globalThis._maskFunction(getBoundingClientRect, 'getBoundingClientRect');
        globalThis._maskFunction(scrollIntoView, 'scrollIntoView');
        globalThis._maskFunction(scrollIntoViewIfNeeded, 'scrollIntoViewIfNeeded');
    }
})();
