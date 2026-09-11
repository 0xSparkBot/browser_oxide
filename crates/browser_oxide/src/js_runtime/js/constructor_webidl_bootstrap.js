// Normalize public DOM/HTML/SVG constructor call/new semantics and WebIDL
// constructor/prototype inheritance to Chromium. Internal lexical classes keep
// owning object creation; public wrappers share their instance prototypes.
((globalThis) => {
    const parentMap = {"Attr":"Node","CharacterData":"Node","Comment":"CharacterData","Document":"Node","DocumentFragment":"Node","DocumentType":"Node","Element":"Node","EventTarget":"Function.prototype","HTMLAnchorElement":"HTMLElement","HTMLAreaElement":"HTMLElement","HTMLAudioElement":"HTMLMediaElement","HTMLBRElement":"HTMLElement","HTMLBaseElement":"HTMLElement","HTMLBodyElement":"HTMLElement","HTMLButtonElement":"HTMLElement","HTMLCanvasElement":"HTMLElement","HTMLDListElement":"HTMLElement","HTMLDataElement":"HTMLElement","HTMLDataListElement":"HTMLElement","HTMLDetailsElement":"HTMLElement","HTMLDialogElement":"HTMLElement","HTMLDirectoryElement":"HTMLElement","HTMLDivElement":"HTMLElement","HTMLDocument":"Document","HTMLElement":"Element","HTMLEmbedElement":"HTMLElement","HTMLFencedFrameElement":"HTMLElement","HTMLFieldSetElement":"HTMLElement","HTMLFontElement":"HTMLElement","HTMLFormElement":"HTMLElement","HTMLFrameElement":"HTMLElement","HTMLFrameSetElement":"HTMLElement","HTMLGeolocationElement":"HTMLElement","HTMLHRElement":"HTMLElement","HTMLHeadElement":"HTMLElement","HTMLHeadingElement":"HTMLElement","HTMLHtmlElement":"HTMLElement","HTMLIFrameElement":"HTMLElement","HTMLImageElement":"HTMLElement","HTMLInputElement":"HTMLElement","HTMLLIElement":"HTMLElement","HTMLLabelElement":"HTMLElement","HTMLLegendElement":"HTMLElement","HTMLLinkElement":"HTMLElement","HTMLMapElement":"HTMLElement","HTMLMarqueeElement":"HTMLElement","HTMLMediaElement":"HTMLElement","HTMLMenuElement":"HTMLElement","HTMLMetaElement":"HTMLElement","HTMLMeterElement":"HTMLElement","HTMLModElement":"HTMLElement","HTMLOListElement":"HTMLElement","HTMLObjectElement":"HTMLElement","HTMLOptGroupElement":"HTMLElement","HTMLOptionElement":"HTMLElement","HTMLOutputElement":"HTMLElement","HTMLParagraphElement":"HTMLElement","HTMLParamElement":"HTMLElement","HTMLPictureElement":"HTMLElement","HTMLPreElement":"HTMLElement","HTMLProgressElement":"HTMLElement","HTMLQuoteElement":"HTMLElement","HTMLScriptElement":"HTMLElement","HTMLSelectElement":"HTMLElement","HTMLSelectedContentElement":"HTMLElement","HTMLSlotElement":"HTMLElement","HTMLSourceElement":"HTMLElement","HTMLSpanElement":"HTMLElement","HTMLStyleElement":"HTMLElement","HTMLTableCaptionElement":"HTMLElement","HTMLTableCellElement":"HTMLElement","HTMLTableColElement":"HTMLElement","HTMLTableElement":"HTMLElement","HTMLTableRowElement":"HTMLElement","HTMLTableSectionElement":"HTMLElement","HTMLTemplateElement":"HTMLElement","HTMLTextAreaElement":"HTMLElement","HTMLTimeElement":"HTMLElement","HTMLTitleElement":"HTMLElement","HTMLTrackElement":"HTMLElement","HTMLUListElement":"HTMLElement","HTMLUnknownElement":"HTMLElement","HTMLVideoElement":"HTMLMediaElement","NamedNodeMap":"Function.prototype","Node":"EventTarget","Range":"AbstractRange","SVGAElement":"SVGGraphicsElement","SVGAnimateElement":"SVGAnimationElement","SVGAnimateMotionElement":"SVGAnimationElement","SVGAnimateTransformElement":"SVGAnimationElement","SVGAnimationElement":"SVGElement","SVGCircleElement":"SVGGeometryElement","SVGClipPathElement":"SVGElement","SVGComponentTransferFunctionElement":"SVGElement","SVGDefsElement":"SVGGraphicsElement","SVGDescElement":"SVGElement","SVGElement":"Element","SVGEllipseElement":"SVGGeometryElement","SVGFEBlendElement":"SVGElement","SVGFEColorMatrixElement":"SVGElement","SVGFEComponentTransferElement":"SVGElement","SVGFECompositeElement":"SVGElement","SVGFEConvolveMatrixElement":"SVGElement","SVGFEDiffuseLightingElement":"SVGElement","SVGFEDisplacementMapElement":"SVGElement","SVGFEDistantLightElement":"SVGElement","SVGFEDropShadowElement":"SVGElement","SVGFEFloodElement":"SVGElement","SVGFEFuncAElement":"SVGComponentTransferFunctionElement","SVGFEFuncBElement":"SVGComponentTransferFunctionElement","SVGFEFuncGElement":"SVGComponentTransferFunctionElement","SVGFEFuncRElement":"SVGComponentTransferFunctionElement","SVGFEGaussianBlurElement":"SVGElement","SVGFEImageElement":"SVGElement","SVGFEMergeElement":"SVGElement","SVGFEMergeNodeElement":"SVGElement","SVGFEMorphologyElement":"SVGElement","SVGFEOffsetElement":"SVGElement","SVGFEPointLightElement":"SVGElement","SVGFESpecularLightingElement":"SVGElement","SVGFESpotLightElement":"SVGElement","SVGFETileElement":"SVGElement","SVGFETurbulenceElement":"SVGElement","SVGFilterElement":"SVGElement","SVGForeignObjectElement":"SVGGraphicsElement","SVGGElement":"SVGGraphicsElement","SVGGeometryElement":"SVGGraphicsElement","SVGGradientElement":"SVGElement","SVGGraphicsElement":"SVGElement","SVGImageElement":"SVGGraphicsElement","SVGLineElement":"SVGGeometryElement","SVGLinearGradientElement":"SVGGradientElement","SVGMPathElement":"SVGElement","SVGMarkerElement":"SVGElement","SVGMaskElement":"SVGElement","SVGMetadataElement":"SVGElement","SVGPathElement":"SVGGeometryElement","SVGPatternElement":"SVGElement","SVGPolygonElement":"SVGGeometryElement","SVGPolylineElement":"SVGGeometryElement","SVGRadialGradientElement":"SVGGradientElement","SVGRectElement":"SVGGeometryElement","SVGSVGElement":"SVGGraphicsElement","SVGScriptElement":"SVGElement","SVGSetElement":"SVGAnimationElement","SVGStopElement":"SVGElement","SVGStyleElement":"SVGElement","SVGSwitchElement":"SVGGraphicsElement","SVGSymbolElement":"SVGGraphicsElement","SVGTSpanElement":"SVGTextPositioningElement","SVGTextContentElement":"SVGGraphicsElement","SVGTextElement":"SVGTextPositioningElement","SVGTextPathElement":"SVGTextContentElement","SVGTextPositioningElement":"SVGTextContentElement","SVGTitleElement":"SVGElement","SVGUseElement":"SVGGraphicsElement","SVGViewElement":"SVGElement","Selection":"Function.prototype","ShadowRoot":"DocumentFragment","Text":"CharacterData"};
    const order = ["EventTarget","NamedNodeMap","Node","Attr","CharacterData","Comment","Document","DocumentFragment","DocumentType","Element","HTMLDocument","HTMLElement","HTMLAnchorElement","HTMLAreaElement","HTMLBRElement","HTMLBaseElement","HTMLBodyElement","HTMLButtonElement","HTMLCanvasElement","HTMLDListElement","HTMLDataElement","HTMLDataListElement","HTMLDetailsElement","HTMLDialogElement","HTMLDirectoryElement","HTMLDivElement","HTMLEmbedElement","HTMLFencedFrameElement","HTMLFieldSetElement","HTMLFontElement","HTMLFormElement","HTMLFrameElement","HTMLFrameSetElement","HTMLGeolocationElement","HTMLHRElement","HTMLHeadElement","HTMLHeadingElement","HTMLHtmlElement","HTMLIFrameElement","HTMLImageElement","HTMLInputElement","HTMLLIElement","HTMLLabelElement","HTMLLegendElement","HTMLLinkElement","HTMLMapElement","HTMLMarqueeElement","HTMLMediaElement","HTMLAudioElement","HTMLMenuElement","HTMLMetaElement","HTMLMeterElement","HTMLModElement","HTMLOListElement","HTMLObjectElement","HTMLOptGroupElement","HTMLOptionElement","HTMLOutputElement","HTMLParagraphElement","HTMLParamElement","HTMLPictureElement","HTMLPreElement","HTMLProgressElement","HTMLQuoteElement","HTMLScriptElement","HTMLSelectElement","HTMLSelectedContentElement","HTMLSlotElement","HTMLSourceElement","HTMLSpanElement","HTMLStyleElement","HTMLTableCaptionElement","HTMLTableCellElement","HTMLTableColElement","HTMLTableElement","HTMLTableRowElement","HTMLTableSectionElement","HTMLTemplateElement","HTMLTextAreaElement","HTMLTimeElement","HTMLTitleElement","HTMLTrackElement","HTMLUListElement","HTMLUnknownElement","HTMLVideoElement","Range","SVGElement","SVGAnimationElement","SVGAnimateElement","SVGAnimateMotionElement","SVGAnimateTransformElement","SVGClipPathElement","SVGComponentTransferFunctionElement","SVGDescElement","SVGFEBlendElement","SVGFEColorMatrixElement","SVGFEComponentTransferElement","SVGFECompositeElement","SVGFEConvolveMatrixElement","SVGFEDiffuseLightingElement","SVGFEDisplacementMapElement","SVGFEDistantLightElement","SVGFEDropShadowElement","SVGFEFloodElement","SVGFEFuncAElement","SVGFEFuncBElement","SVGFEFuncGElement","SVGFEFuncRElement","SVGFEGaussianBlurElement","SVGFEImageElement","SVGFEMergeElement","SVGFEMergeNodeElement","SVGFEMorphologyElement","SVGFEOffsetElement","SVGFEPointLightElement","SVGFESpecularLightingElement","SVGFESpotLightElement","SVGFETileElement","SVGFETurbulenceElement","SVGFilterElement","SVGGradientElement","SVGGraphicsElement","SVGAElement","SVGDefsElement","SVGForeignObjectElement","SVGGElement","SVGGeometryElement","SVGCircleElement","SVGEllipseElement","SVGImageElement","SVGLineElement","SVGLinearGradientElement","SVGMPathElement","SVGMarkerElement","SVGMaskElement","SVGMetadataElement","SVGPathElement","SVGPatternElement","SVGPolygonElement","SVGPolylineElement","SVGRadialGradientElement","SVGRectElement","SVGSVGElement","SVGScriptElement","SVGSetElement","SVGStopElement","SVGStyleElement","SVGSwitchElement","SVGSymbolElement","SVGTextContentElement","SVGTextPathElement","SVGTextPositioningElement","SVGTSpanElement","SVGTextElement","SVGTitleElement","SVGUseElement","SVGViewElement","Selection","ShadowRoot","Text"];
    const constructible = new Set(["Comment","Document","DocumentFragment","EventTarget","Range","Text"]);
    const simpleDirectIllegal = new Set(["Attr","CharacterData","DocumentType","Element","HTMLDocument","HTMLMediaElement","HTMLUnknownElement","NamedNodeMap","Node","SVGAElement","SVGAnimateElement","SVGAnimateMotionElement","SVGAnimateTransformElement","SVGAnimationElement","SVGCircleElement","SVGClipPathElement","SVGComponentTransferFunctionElement","SVGDefsElement","SVGDescElement","SVGElement","SVGEllipseElement","SVGFEBlendElement","SVGFEColorMatrixElement","SVGFEComponentTransferElement","SVGFECompositeElement","SVGFEConvolveMatrixElement","SVGFEDiffuseLightingElement","SVGFEDisplacementMapElement","SVGFEDistantLightElement","SVGFEDropShadowElement","SVGFEFloodElement","SVGFEFuncAElement","SVGFEFuncBElement","SVGFEFuncGElement","SVGFEFuncRElement","SVGFEGaussianBlurElement","SVGFEImageElement","SVGFEMergeElement","SVGFEMergeNodeElement","SVGFEMorphologyElement","SVGFEOffsetElement","SVGFEPointLightElement","SVGFESpecularLightingElement","SVGFESpotLightElement","SVGFETileElement","SVGFETurbulenceElement","SVGFilterElement","SVGForeignObjectElement","SVGGElement","SVGGeometryElement","SVGGradientElement","SVGGraphicsElement","SVGImageElement","SVGLineElement","SVGLinearGradientElement","SVGMPathElement","SVGMarkerElement","SVGMaskElement","SVGMetadataElement","SVGPathElement","SVGPatternElement","SVGPolygonElement","SVGPolylineElement","SVGRadialGradientElement","SVGRectElement","SVGSVGElement","SVGScriptElement","SVGSetElement","SVGStopElement","SVGStyleElement","SVGSwitchElement","SVGSymbolElement","SVGTSpanElement","SVGTextContentElement","SVGTextElement","SVGTextPathElement","SVGTextPositioningElement","SVGTitleElement","SVGUseElement","SVGViewElement","Selection","ShadowRoot"]);
    const originals = new Map();
    const wrappers = new Map();

    for (const name of order) {
        let original;
        try { original = globalThis[name]; } catch (_) { continue; }
        if (typeof original !== 'function' || !original.prototype) continue;
        originals.set(name, original);
    }

    const requireNewMessage = name =>
        `Failed to construct '${name}': Please use the 'new' operator, this DOM object constructor cannot be called as a function.`;
    const illegalNewMessage = name => `Failed to construct '${name}': Illegal constructor`;
    const mask = (fn, name) => {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };

    for (const name of order) {
        const original = originals.get(name);
        if (!original) continue;
        const canConstruct = constructible.has(name);
        const simpleDirect = simpleDirectIllegal.has(name);
        const Wrapper = function() {
            if (!new.target) {
                if (simpleDirect) throw new TypeError('Illegal constructor');
                throw new TypeError(requireNewMessage(name));
            }
            if (!canConstruct) throw new TypeError(illegalNewMessage(name));
            return Reflect.construct(original, Array.from(arguments), new.target);
        };
        try { Object.defineProperty(Wrapper, 'length', { value: original.length, configurable: true }); } catch (_) {}
        mask(Wrapper, name);
        try { Wrapper.prototype = original.prototype; } catch (_) {}

        for (const key of Object.getOwnPropertyNames(original)) {
            if (key === 'length' || key === 'name' || key === 'prototype' || key === 'caller' || key === 'arguments') continue;
            try {
                const d = Object.getOwnPropertyDescriptor(original, key);
                if (d) Object.defineProperty(Wrapper, key, d);
            } catch (_) {}
        }
        try {
            Object.defineProperty(original.prototype, 'constructor', {
                value: Wrapper, writable: true, enumerable: false, configurable: true,
            });
        } catch (_) {}
        wrappers.set(name, Wrapper);
        globalThis[name] = Wrapper;
    }

    for (const name of order) {
        const Wrapper = wrappers.get(name);
        if (!Wrapper) continue;
        const parentName = parentMap[name];
        if (parentName === 'Function.prototype') {
            try { Object.setPrototypeOf(Wrapper, Function.prototype); } catch (_) {}
            continue;
        }
        let parent;
        try { parent = wrappers.get(parentName) || globalThis[parentName]; } catch (_) { parent = null; }
        if (typeof parent !== 'function') continue;
        try { Object.setPrototypeOf(Wrapper, parent); } catch (_) {}
        try {
            if (Wrapper.prototype && parent.prototype && Object.getPrototypeOf(Wrapper.prototype) !== parent.prototype) {
                Object.setPrototypeOf(Wrapper.prototype, parent.prototype);
            }
        } catch (_) {}
    }

    // Reconnect constructors outside this DOM/HTML/SVG wrapper set that
    // directly inherited a constructor we replaced (most notably EventTarget
    // descendants such as MediaQueryList, Navigation and WebRTC classes).
    // Their instance prototype chain already points at the shared old/new base
    // prototype, so only the constructor [[Prototype]] needs retargeting.
    const replacementByOriginal = new Map();
    for (const [name, original] of originals) {
        const replacement = wrappers.get(name);
        if (replacement) replacementByOriginal.set(original, replacement);
    }
    for (const key of Object.getOwnPropertyNames(globalThis)) {
        let candidate;
        try { candidate = globalThis[key]; } catch (_) { continue; }
        if (typeof candidate !== 'function' || wrappers.has(key)) continue;
        try {
            const replacementParent = replacementByOriginal.get(Object.getPrototypeOf(candidate));
            if (replacementParent) Object.setPrototypeOf(candidate, replacementParent);
        } catch (_) {}
    }
})(globalThis);

// Chrome 148 constructor statics not provided by the lightweight interface
// stubs. Keep these outside the wrapper loop so own-property order is
// length/name/prototype followed by the WebIDL statics, matching Chromium.
((globalThis) => {
    const mask = (fn, name, length) => {
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            Object.defineProperty(fn, 'length', { value: length, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };
    const method = (Ctor, name, fn, length) => {
        if (typeof Ctor !== 'function') return;
        Object.defineProperty(Ctor, name, {
            value: mask(fn, name, length), writable: true, enumerable: true, configurable: true,
        });
    };
    const constants = (Ctor, entries) => {
        if (typeof Ctor !== 'function') return;
        for (const [name, value] of entries) {
            if (Object.prototype.hasOwnProperty.call(Ctor, name)) continue;
            Object.defineProperty(Ctor, name, {
                value, writable: false, enumerable: true, configurable: false,
            });
        }
    };

    const parseDetachedHTML = (input, sanitize) => {
        const source = String(input);
        const doctypeMatch = source.match(
            /<!doctype\s+([^\s>]+)(?:\s+PUBLIC\s+["']([^"']*)["']\s+["']([^"']*)["']|\s+SYSTEM\s+["']([^"']*)["'])?\s*>/i
        );
        const htmlElement = globalThis.document.createElement('html');
        const head = globalThis.document.createElement('head');
        const body = globalThis.document.createElement('body');
        htmlElement.appendChild(head);
        htmlElement.appendChild(body);

        const headMatch = source.match(/<head(?:\s[^>]*)?>([\s\S]*?)<\/head\s*>/i);
        const bodyMatch = source.match(/<body(?:\s[^>]*)?>([\s\S]*?)<\/body\s*>/i);
        const titleMatch = source.match(/<title(?:\s[^>]*)?>([\s\S]*?)<\/title\s*>/i);
        let headHTML = headMatch ? headMatch[1] : (titleMatch ? titleMatch[0] : '');
        let bodyHTML;
        if (bodyMatch) {
            bodyHTML = bodyMatch[1];
        } else {
            bodyHTML = source
                .replace(/<!doctype[^>]*>/ig, '')
                .replace(/<\/?html(?:\s[^>]*)?>/ig, '')
                .replace(/<head(?:\s[^>]*)?>[\s\S]*?<\/head\s*>/ig, '')
                .replace(/<title(?:\s[^>]*)?>[\s\S]*?<\/title\s*>/ig, '');
        }
        head.innerHTML = headHTML;
        body.innerHTML = bodyHTML;

        if (sanitize) {
            for (const script of Array.from(htmlElement.querySelectorAll('script'))) script.remove();
            for (const el of Array.from(htmlElement.querySelectorAll('*'))) {
                for (const attr of el.getAttributeNames ? el.getAttributeNames() : []) el.removeAttribute(attr);
            }
        }

        const target = Object.create(globalThis.HTMLDocument?.prototype || globalThis.Document.prototype);
        Object.defineProperty(target, 'location', {
            value: null, writable: false, enumerable: false, configurable: true,
        });
        const detachedDoctype = doctypeMatch && globalThis.document?.implementation?.createDocumentType
            ? globalThis.document.implementation.createDocumentType(
                doctypeMatch[1],
                doctypeMatch[2] || '',
                doctypeMatch[3] || doctypeMatch[4] || ''
            )
            : null;
        const getTitle = () => head.querySelector('title')?.textContent || '';
        const setTitle = value => {
            let title = head.querySelector('title');
            if (!title) { title = globalThis.document.createElement('title'); head.appendChild(title); }
            title.textContent = String(value);
        };
        const getById = id => {
            const wanted = String(id);
            const all = htmlElement.querySelectorAll('*');
            for (let i = 0; i < all.length; i++) if (all[i].id === wanted) return all[i];
            return null;
        };
        const bindings = {
            querySelector: selector => htmlElement.querySelector(selector),
            querySelectorAll: selector => htmlElement.querySelectorAll(selector),
            getElementById: getById,
            getElementsByTagName: tag => htmlElement.getElementsByTagName(tag),
            getElementsByClassName: cls => htmlElement.getElementsByClassName(cls),
            createElement: tag => globalThis.document.createElement(tag),
            createElementNS: (ns, tag) => globalThis.document.createElementNS(ns, tag),
            createTextNode: data => globalThis.document.createTextNode(data),
            createDocumentFragment: () => globalThis.document.createDocumentFragment(),
        };
        return new Proxy(target, {
            get(object, key, receiver) {
                if (key === 'documentElement') return htmlElement;
                if (key === 'doctype') return detachedDoctype;
                if (key === 'head') return head;
                if (key === 'body') return body;
                if (key === 'title') return getTitle();
                if (key === 'URL' || key === 'documentURI' || key === 'baseURI') return 'about:blank';
                if (key === 'defaultView' || key === 'location') return null;
                if (key === 'contentType') return 'text/html';
                if (key === 'characterSet' || key === 'charset' || key === 'inputEncoding') return 'UTF-8';
                if (key === 'scripts') return htmlElement.querySelectorAll('script');
                if (key === 'forms') return htmlElement.querySelectorAll('form');
                if (key === 'images') return htmlElement.querySelectorAll('img');
                if (Object.prototype.hasOwnProperty.call(bindings, key)) return bindings[key];
                return Reflect.get(object, key, receiver);
            },
            set(object, key, value, receiver) {
                if (key === 'title') { setTitle(value); return true; }
                return Reflect.set(object, key, value, receiver);
            },
        });
    };

    // DocumentType implements ChildNode. The lexical DOM class owns the
    // behavior; normalize the public prototype to Chromium's WebIDL member
    // order and descriptor shape after constructor wrapping.
    if (globalThis.DocumentType && globalThis.DocumentType.prototype) {
        const proto = globalThis.DocumentType.prototype;
        const ctor = globalThis.DocumentType;
        const saved = {};
        for (const key of ['name', 'publicId', 'systemId', 'after', 'before', 'remove', 'replaceWith']) {
            saved[key] = Object.getOwnPropertyDescriptor(proto, key);
            try { delete proto[key]; } catch (_) {}
        }
        try { delete proto.constructor; } catch (_) {}

        for (const key of ['name', 'publicId', 'systemId']) {
            const descriptor = saved[key];
            if (descriptor) {
                Object.defineProperty(proto, key, {
                    ...descriptor, enumerable: true, configurable: true,
                });
            }
        }
        for (const key of ['after', 'before', 'remove', 'replaceWith']) {
            const descriptor = saved[key];
            if (!descriptor || typeof descriptor.value !== 'function') continue;
            mask(descriptor.value, key, 0);
            Object.defineProperty(proto, key, {
                value: descriptor.value,
                writable: true,
                enumerable: true,
                configurable: true,
            });
        }
        Object.defineProperty(proto, 'constructor', {
            value: ctor, writable: true, enumerable: false, configurable: true,
        });
        Object.defineProperty(proto, Symbol.toStringTag, {
            value: 'DocumentType', writable: false, enumerable: false, configurable: true,
        });
        const unscopables = Object.create(null);
        for (const key of ['after', 'before', 'remove', 'replaceWith']) unscopables[key] = true;
        Object.defineProperty(proto, Symbol.unscopables, {
            value: unscopables, writable: false, enumerable: false, configurable: true,
        });
    }

    method(globalThis.Document, 'parseHTMLUnsafe', function parseHTMLUnsafe(html) {
        return parseDetachedHTML(html, false);
    }, 1);
    method(globalThis.Document, 'parseHTML', function parseHTML(html) {
        return parseDetachedHTML(html, true);
    }, 1);
    method(globalThis.HTMLFencedFrameElement, 'canLoadOpaqueURL', function canLoadOpaqueURL() {
        return true;
    }, 0);
    method(globalThis.HTMLScriptElement, 'supports', function supports(type) {
        return new Set(['classic', 'module', 'importmap', 'speculationrules', 'webbundle']).has(String(type));
    }, 1);

    constants(globalThis.HTMLMediaElement, [
        ['NETWORK_EMPTY',0],['NETWORK_IDLE',1],['NETWORK_LOADING',2],['NETWORK_NO_SOURCE',3],
        ['HAVE_NOTHING',0],['HAVE_METADATA',1],['HAVE_CURRENT_DATA',2],['HAVE_FUTURE_DATA',3],['HAVE_ENOUGH_DATA',4],
    ]);
    constants(globalThis.HTMLTrackElement, [['NONE',0],['LOADING',1],['LOADED',2],['ERROR',3]]);
    constants(globalThis.SVGComponentTransferFunctionElement, [
        ['SVG_FECOMPONENTTRANSFER_TYPE_UNKNOWN',0],['SVG_FECOMPONENTTRANSFER_TYPE_IDENTITY',1],
        ['SVG_FECOMPONENTTRANSFER_TYPE_TABLE',2],['SVG_FECOMPONENTTRANSFER_TYPE_DISCRETE',3],
        ['SVG_FECOMPONENTTRANSFER_TYPE_LINEAR',4],['SVG_FECOMPONENTTRANSFER_TYPE_GAMMA',5],
    ]);
    constants(globalThis.SVGFEBlendElement, [
        ['SVG_FEBLEND_MODE_UNKNOWN',0],['SVG_FEBLEND_MODE_NORMAL',1],['SVG_FEBLEND_MODE_MULTIPLY',2],
        ['SVG_FEBLEND_MODE_SCREEN',3],['SVG_FEBLEND_MODE_DARKEN',4],['SVG_FEBLEND_MODE_LIGHTEN',5],
        ['SVG_FEBLEND_MODE_OVERLAY',6],['SVG_FEBLEND_MODE_COLOR_DODGE',7],['SVG_FEBLEND_MODE_COLOR_BURN',8],
        ['SVG_FEBLEND_MODE_HARD_LIGHT',9],['SVG_FEBLEND_MODE_SOFT_LIGHT',10],['SVG_FEBLEND_MODE_DIFFERENCE',11],
        ['SVG_FEBLEND_MODE_EXCLUSION',12],['SVG_FEBLEND_MODE_HUE',13],['SVG_FEBLEND_MODE_SATURATION',14],
        ['SVG_FEBLEND_MODE_COLOR',15],['SVG_FEBLEND_MODE_LUMINOSITY',16],
    ]);
    constants(globalThis.SVGFEColorMatrixElement, [
        ['SVG_FECOLORMATRIX_TYPE_UNKNOWN',0],['SVG_FECOLORMATRIX_TYPE_MATRIX',1],
        ['SVG_FECOLORMATRIX_TYPE_SATURATE',2],['SVG_FECOLORMATRIX_TYPE_HUEROTATE',3],
        ['SVG_FECOLORMATRIX_TYPE_LUMINANCETOALPHA',4],
    ]);
    constants(globalThis.SVGFECompositeElement, [
        ['SVG_FECOMPOSITE_OPERATOR_UNKNOWN',0],['SVG_FECOMPOSITE_OPERATOR_OVER',1],
        ['SVG_FECOMPOSITE_OPERATOR_IN',2],['SVG_FECOMPOSITE_OPERATOR_OUT',3],
        ['SVG_FECOMPOSITE_OPERATOR_ATOP',4],['SVG_FECOMPOSITE_OPERATOR_XOR',5],
        ['SVG_FECOMPOSITE_OPERATOR_ARITHMETIC',6],
    ]);
    constants(globalThis.SVGFEConvolveMatrixElement, [
        ['SVG_EDGEMODE_UNKNOWN',0],['SVG_EDGEMODE_DUPLICATE',1],['SVG_EDGEMODE_WRAP',2],['SVG_EDGEMODE_NONE',3],
    ]);
    constants(globalThis.SVGFEDisplacementMapElement, [
        ['SVG_CHANNEL_UNKNOWN',0],['SVG_CHANNEL_R',1],['SVG_CHANNEL_G',2],['SVG_CHANNEL_B',3],['SVG_CHANNEL_A',4],
    ]);
    constants(globalThis.SVGFEMorphologyElement, [
        ['SVG_MORPHOLOGY_OPERATOR_UNKNOWN',0],['SVG_MORPHOLOGY_OPERATOR_ERODE',1],['SVG_MORPHOLOGY_OPERATOR_DILATE',2],
    ]);
    constants(globalThis.SVGFETurbulenceElement, [
        ['SVG_TURBULENCE_TYPE_UNKNOWN',0],['SVG_TURBULENCE_TYPE_FRACTALNOISE',1],['SVG_TURBULENCE_TYPE_TURBULENCE',2],
        ['SVG_STITCHTYPE_UNKNOWN',0],['SVG_STITCHTYPE_STITCH',1],['SVG_STITCHTYPE_NOSTITCH',2],
    ]);
    constants(globalThis.SVGGradientElement, [
        ['SVG_SPREADMETHOD_UNKNOWN',0],['SVG_SPREADMETHOD_PAD',1],['SVG_SPREADMETHOD_REFLECT',2],['SVG_SPREADMETHOD_REPEAT',3],
    ]);
    constants(globalThis.SVGMarkerElement, [
        ['SVG_MARKERUNITS_UNKNOWN',0],['SVG_MARKERUNITS_USERSPACEONUSE',1],['SVG_MARKERUNITS_STROKEWIDTH',2],
        ['SVG_MARKER_ORIENT_UNKNOWN',0],['SVG_MARKER_ORIENT_AUTO',1],['SVG_MARKER_ORIENT_ANGLE',2],
    ]);
    for (const name of ['SVGSVGElement','SVGViewElement']) {
        constants(globalThis[name], [['SVG_ZOOMANDPAN_UNKNOWN',0],['SVG_ZOOMANDPAN_DISABLE',1],['SVG_ZOOMANDPAN_MAGNIFY',2]]);
    }
    constants(globalThis.SVGTextContentElement, [
        ['LENGTHADJUST_UNKNOWN',0],['LENGTHADJUST_SPACING',1],['LENGTHADJUST_SPACINGANDGLYPHS',2],
    ]);
    constants(globalThis.SVGTextPathElement, [
        ['TEXTPATH_METHODTYPE_UNKNOWN',0],['TEXTPATH_METHODTYPE_ALIGN',1],['TEXTPATH_METHODTYPE_STRETCH',2],
        ['TEXTPATH_SPACINGTYPE_UNKNOWN',0],['TEXTPATH_SPACINGTYPE_AUTO',1],['TEXTPATH_SPACINGTYPE_EXACT',2],
    ]);

    // This one legacy class lacked its own WebIDL brand while all other
    // constructor-prototype tags already matched Chromium.
    try {
        Object.defineProperty(globalThis.HTMLCanvasElement.prototype, Symbol.toStringTag, {
            value: 'HTMLCanvasElement', configurable: true,
        });
    } catch (_) {}
})(globalThis);
