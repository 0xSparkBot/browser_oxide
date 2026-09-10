// CSSOM compatibility layer.
//
// The lower DOM bootstrap already provides live style-attribute storage and
// constructable stylesheet plumbing.  This file keeps those behaviors, while
// moving observable state behind WeakMaps/Proxies and exposing Chromium-like
// WebIDL prototypes for the core CSSOM interfaces.
((globalThis) => {
    const mask = (fn, name) => {
        try {
            if (typeof _maskFunction === 'function' && typeof fn === 'function') {
                _maskFunction(fn, name);
            }
        } catch (_) {}
        return fn;
    };
    const setLength = (fn, length) => {
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
    };
    const defineMethod = (proto, name, fn, length) => {
        setLength(fn, length);
        mask(fn, name);
        Object.defineProperty(proto, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };
    const defineGetter = (proto, name, getter, setter) => {
        mask(getter, `get ${name}`);
        if (setter) mask(setter, `set ${name}`);
        Object.defineProperty(proto, name, {
            get: getter, set: setter,
            enumerable: true, configurable: true,
        });
    };
    const defineTag = (proto, tag) => {
        try {
            Object.defineProperty(proto, Symbol.toStringTag, {
                value: tag, writable: false, enumerable: false, configurable: true,
            });
        } catch (_) {}
    };
    const illegal = (name) => {
        const Ctor = function() {
            throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
        };
        try { Object.defineProperty(Ctor, 'name', { value: name, configurable: true }); } catch (_) {}
        setLength(Ctor, 0);
        mask(Ctor, name);
        return Ctor;
    };

    // ------------------------------------------------------------
    // CSSStyleDeclaration
    // ------------------------------------------------------------
    const declarationState = new WeakMap();

    const parseDeclaration = (text) => {
        const out = [];
        for (const rawPart of String(text || '').split(';')) {
            const idx = rawPart.indexOf(':');
            if (idx < 0) continue;
            const name = rawPart.slice(0, idx).trim().toLowerCase();
            if (!name) continue;
            let value = rawPart.slice(idx + 1).trim();
            let priority = '';
            const important = /\s*!important\s*$/i.exec(value);
            if (important) {
                value = value.slice(0, important.index).trim();
                priority = 'important';
            }
            const prior = out.findIndex((entry) => entry.name === name);
            const entry = { name, value, priority };
            if (prior >= 0) out[prior] = entry;
            else out.push(entry);
        }
        return out;
    };
    const serializeDeclaration = (entries) => entries
        .filter((entry) => entry.value !== '')
        .map((entry) => `${entry.name}: ${entry.value}${entry.priority ? ' !important' : ''};`)
        .join(' ');

    let CSSStyleDeclaration = illegal('CSSStyleDeclaration');
    CSSStyleDeclaration.prototype = Object.create(Object.prototype);
    Object.defineProperty(CSSStyleDeclaration.prototype, 'constructor', {
        value: CSSStyleDeclaration, writable: true, enumerable: false, configurable: true,
    });

    const declarationEntries = (self) => {
        const state = declarationState.get(self);
        if (!state) throw new TypeError('Illegal invocation');
        return parseDeclaration(state.getText());
    };
    const declarationCommit = (self, entries) => {
        const state = declarationState.get(self);
        if (!state) throw new TypeError('Illegal invocation');
        state.setText(serializeDeclaration(entries));
    };

    defineGetter(CSSStyleDeclaration.prototype, 'cssText',
        function cssText() {
            const state = declarationState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return serializeDeclaration(parseDeclaration(state.getText()));
        },
        function cssText(value) {
            const state = declarationState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            state.setText(serializeDeclaration(parseDeclaration(String(value))));
        });
    defineGetter(CSSStyleDeclaration.prototype, 'length', function length() {
        return declarationEntries(this).length;
    });
    defineGetter(CSSStyleDeclaration.prototype, 'parentRule', function parentRule() {
        const state = declarationState.get(this);
        if (!state) throw new TypeError('Illegal invocation');
        return state.parentRule || null;
    });
    defineGetter(CSSStyleDeclaration.prototype, 'cssFloat',
        function cssFloat() { return CSSStyleDeclaration.prototype.getPropertyValue.call(this, 'float'); },
        function cssFloat(value) { CSSStyleDeclaration.prototype.setProperty.call(this, 'float', value, ''); });
    defineMethod(CSSStyleDeclaration.prototype, 'item', function item(index) {
        const entries = declarationEntries(this);
        const i = Number(index) >>> 0;
        return entries[i]?.name || '';
    }, 1);
    defineMethod(CSSStyleDeclaration.prototype, 'getPropertyValue', function getPropertyValue(name) {
        const wanted = String(name).toLowerCase();
        return declarationEntries(this).find((entry) => entry.name === wanted)?.value || '';
    }, 1);
    defineMethod(CSSStyleDeclaration.prototype, 'getPropertyPriority', function getPropertyPriority(name) {
        const wanted = String(name).toLowerCase();
        return declarationEntries(this).find((entry) => entry.name === wanted)?.priority || '';
    }, 1);
    defineMethod(CSSStyleDeclaration.prototype, 'setProperty', function setProperty(name, value) {
        const priority = arguments.length > 2 ? String(arguments[2]).toLowerCase() : '';
        if (priority !== '' && priority !== 'important') return;
        const wanted = String(name).toLowerCase();
        if (!wanted) return;
        const entries = declarationEntries(this);
        const next = { name: wanted, value: String(value), priority };
        const index = entries.findIndex((entry) => entry.name === wanted);
        if (index >= 0) entries[index] = next;
        else entries.push(next);
        declarationCommit(this, entries);
    }, 2);
    defineMethod(CSSStyleDeclaration.prototype, 'removeProperty', function removeProperty(name) {
        const wanted = String(name).toLowerCase();
        const entries = declarationEntries(this);
        const index = entries.findIndex((entry) => entry.name === wanted);
        if (index < 0) return '';
        const [removed] = entries.splice(index, 1);
        declarationCommit(this, entries);
        return removed.value;
    }, 1);
    defineTag(CSSStyleDeclaration.prototype, 'CSSStyleDeclaration');

    const declarationCache = new WeakMap();
    // Chromium exposes the supported CSS IDL attributes as legacy named own
    // properties on every CSSStyleDeclaration.  Numeric declaration indices
    // come first, followed by this fixed Blink order.
    const cssIdlOwnNames = ["accentColor","additiveSymbols","alignContent","alignItems","alignSelf","alignmentBaseline","all","anchorName","anchorScope","animation","animationComposition","animationDelay","animationDirection","animationDuration","animationFillMode","animationIterationCount","animationName","animationPlayState","animationRange","animationRangeEnd","animationRangeStart","animationTimeline","animationTimingFunction","animationTrigger","appRegion","appearance","ascentOverride","aspectRatio","backdropFilter","backfaceVisibility","background","backgroundAttachment","backgroundBlendMode","backgroundClip","backgroundColor","backgroundImage","backgroundOrigin","backgroundPosition","backgroundPositionX","backgroundPositionY","backgroundRepeat","backgroundSize","basePalette","baselineShift","baselineSource","blockSize","border","borderBlock","borderBlockColor","borderBlockEnd","borderBlockEndColor","borderBlockEndStyle","borderBlockEndWidth","borderBlockStart","borderBlockStartColor","borderBlockStartStyle","borderBlockStartWidth","borderBlockStyle","borderBlockWidth","borderBottom","borderBottomColor","borderBottomLeftRadius","borderBottomRightRadius","borderBottomStyle","borderBottomWidth","borderCollapse","borderColor","borderEndEndRadius","borderEndStartRadius","borderImage","borderImageOutset","borderImageRepeat","borderImageSlice","borderImageSource","borderImageWidth","borderInline","borderInlineColor","borderInlineEnd","borderInlineEndColor","borderInlineEndStyle","borderInlineEndWidth","borderInlineStart","borderInlineStartColor","borderInlineStartStyle","borderInlineStartWidth","borderInlineStyle","borderInlineWidth","borderLeft","borderLeftColor","borderLeftStyle","borderLeftWidth","borderRadius","borderRight","borderRightColor","borderRightStyle","borderRightWidth","borderShape","borderSpacing","borderStartEndRadius","borderStartStartRadius","borderStyle","borderTop","borderTopColor","borderTopLeftRadius","borderTopRightRadius","borderTopStyle","borderTopWidth","borderWidth","bottom","boxDecorationBreak","boxShadow","boxSizing","breakAfter","breakBefore","breakInside","bufferedRendering","captionSide","caretAnimation","caretColor","caretShape","clear","clip","clipPath","clipRule","color","colorInterpolation","colorInterpolationFilters","colorRendering","colorScheme","columnCount","columnFill","columnGap","columnHeight","columnRule","columnRuleColor","columnRuleStyle","columnRuleWidth","columnSpan","columnWidth","columnWrap","columns","contain","containIntrinsicBlockSize","containIntrinsicHeight","containIntrinsicInlineSize","containIntrinsicSize","containIntrinsicWidth","container","containerName","containerType","content","contentVisibility","cornerBlockEndShape","cornerBlockStartShape","cornerBottomLeftShape","cornerBottomRightShape","cornerBottomShape","cornerEndEndShape","cornerEndStartShape","cornerInlineEndShape","cornerInlineStartShape","cornerLeftShape","cornerRightShape","cornerShape","cornerStartEndShape","cornerStartStartShape","cornerTopLeftShape","cornerTopRightShape","cornerTopShape","counterIncrement","counterReset","counterSet","cursor","cx","cy","d","descentOverride","direction","display","dominantBaseline","dynamicRangeLimit","emptyCells","epubCaptionSide","epubTextCombine","epubTextEmphasis","epubTextEmphasisColor","epubTextEmphasisStyle","epubTextOrientation","epubTextTransform","epubWordBreak","epubWritingMode","fallback","fieldSizing","fill","fillOpacity","fillRule","filter","flex","flexBasis","flexDirection","flexFlow","flexGrow","flexShrink","flexWrap","float","floodColor","floodOpacity","font","fontDisplay","fontFamily","fontFeatureSettings","fontKerning","fontLanguageOverride","fontOpticalSizing","fontPalette","fontSize","fontSizeAdjust","fontStretch","fontStyle","fontSynthesis","fontSynthesisSmallCaps","fontSynthesisStyle","fontSynthesisWeight","fontVariant","fontVariantAlternates","fontVariantCaps","fontVariantEastAsian","fontVariantEmoji","fontVariantLigatures","fontVariantNumeric","fontVariantPosition","fontVariationSettings","fontWeight","forcedColorAdjust","gap","grid","gridArea","gridAutoColumns","gridAutoFlow","gridAutoRows","gridColumn","gridColumnEnd","gridColumnGap","gridColumnStart","gridGap","gridRow","gridRowEnd","gridRowGap","gridRowStart","gridTemplate","gridTemplateAreas","gridTemplateColumns","gridTemplateRows","height","hyphenateCharacter","hyphenateLimitChars","hyphens","imageOrientation","imageRendering","inherits","initialLetter","initialValue","inlineSize","inset","insetBlock","insetBlockEnd","insetBlockStart","insetInline","insetInlineEnd","insetInlineStart","interactivity","interestDelay","interestDelayEnd","interestDelayStart","interpolateSize","isolation","justifyContent","justifyItems","justifySelf","left","letterSpacing","lightingColor","lineBreak","lineGapOverride","lineHeight","listStyle","listStyleImage","listStylePosition","listStyleType","margin","marginBlock","marginBlockEnd","marginBlockStart","marginBottom","marginInline","marginInlineEnd","marginInlineStart","marginLeft","marginRight","marginTop","marker","markerEnd","markerMid","markerStart","mask","maskClip","maskComposite","maskImage","maskMode","maskOrigin","maskPosition","maskRepeat","maskSize","maskType","mathDepth","mathShift","mathStyle","maxBlockSize","maxHeight","maxInlineSize","maxWidth","minBlockSize","minHeight","minInlineSize","minWidth","mixBlendMode","navigation","negative","objectFit","objectPosition","objectViewBox","offset","offsetAnchor","offsetDistance","offsetPath","offsetPosition","offsetRotate","opacity","order","orphans","outline","outlineColor","outlineOffset","outlineStyle","outlineWidth","overflow","overflowAnchor","overflowBlock","overflowClipMargin","overflowInline","overflowWrap","overflowX","overflowY","overlay","overrideColors","overscrollBehavior","overscrollBehaviorBlock","overscrollBehaviorInline","overscrollBehaviorX","overscrollBehaviorY","pad","padding","paddingBlock","paddingBlockEnd","paddingBlockStart","paddingBottom","paddingInline","paddingInlineEnd","paddingInlineStart","paddingLeft","paddingRight","paddingTop","page","pageBreakAfter","pageBreakBefore","pageBreakInside","pageOrientation","paintOrder","perspective","perspectiveOrigin","placeContent","placeItems","placeSelf","pointerEvents","position","positionAnchor","positionArea","positionTry","positionTryFallbacks","positionTryOrder","positionVisibility","prefix","printColorAdjust","quotes","r","range","readingFlow","readingOrder","resize","result","right","rotate","rowGap","rubyAlign","rubyPosition","rx","ry","scale","scrollBehavior","scrollInitialTarget","scrollMargin","scrollMarginBlock","scrollMarginBlockEnd","scrollMarginBlockStart","scrollMarginBottom","scrollMarginInline","scrollMarginInlineEnd","scrollMarginInlineStart","scrollMarginLeft","scrollMarginRight","scrollMarginTop","scrollMarkerGroup","scrollPadding","scrollPaddingBlock","scrollPaddingBlockEnd","scrollPaddingBlockStart","scrollPaddingBottom","scrollPaddingInline","scrollPaddingInlineEnd","scrollPaddingInlineStart","scrollPaddingLeft","scrollPaddingRight","scrollPaddingTop","scrollSnapAlign","scrollSnapStop","scrollSnapType","scrollTargetGroup","scrollTimeline","scrollTimelineAxis","scrollTimelineName","scrollbarColor","scrollbarGutter","scrollbarWidth","shapeImageThreshold","shapeMargin","shapeOutside","shapeRendering","size","sizeAdjust","speak","speakAs","src","stopColor","stopOpacity","stroke","strokeDasharray","strokeDashoffset","strokeLinecap","strokeLinejoin","strokeMiterlimit","strokeOpacity","strokeWidth","suffix","symbols","syntax","system","tabSize","tableLayout","textAlign","textAlignLast","textAnchor","textAutospace","textBox","textBoxEdge","textBoxTrim","textCombineUpright","textDecoration","textDecorationColor","textDecorationLine","textDecorationSkipInk","textDecorationStyle","textDecorationThickness","textEmphasis","textEmphasisColor","textEmphasisPosition","textEmphasisStyle","textIndent","textJustify","textOrientation","textOverflow","textRendering","textShadow","textSizeAdjust","textSpacingTrim","textTransform","textUnderlineOffset","textUnderlinePosition","textWrap","textWrapMode","textWrapStyle","timelineScope","timelineTrigger","timelineTriggerActivationRange","timelineTriggerActivationRangeEnd","timelineTriggerActivationRangeStart","timelineTriggerActiveRange","timelineTriggerActiveRangeEnd","timelineTriggerActiveRangeStart","timelineTriggerName","timelineTriggerSource","top","touchAction","transform","transformBox","transformOrigin","transformStyle","transition","transitionBehavior","transitionDelay","transitionDuration","transitionProperty","transitionTimingFunction","translate","triggerScope","types","unicodeBidi","unicodeRange","userSelect","vectorEffect","verticalAlign","viewTimeline","viewTimelineAxis","viewTimelineInset","viewTimelineName","viewTransitionClass","viewTransitionGroup","viewTransitionName","viewTransitionScope","visibility","webkitAlignContent","webkitAlignItems","webkitAlignSelf","webkitAnimation","webkitAnimationDelay","webkitAnimationDirection","webkitAnimationDuration","webkitAnimationFillMode","webkitAnimationIterationCount","webkitAnimationName","webkitAnimationPlayState","webkitAnimationTimingFunction","webkitAppRegion","webkitAppearance","webkitBackfaceVisibility","webkitBackgroundClip","webkitBackgroundOrigin","webkitBackgroundSize","webkitBorderAfter","webkitBorderAfterColor","webkitBorderAfterStyle","webkitBorderAfterWidth","webkitBorderBefore","webkitBorderBeforeColor","webkitBorderBeforeStyle","webkitBorderBeforeWidth","webkitBorderBottomLeftRadius","webkitBorderBottomRightRadius","webkitBorderEnd","webkitBorderEndColor","webkitBorderEndStyle","webkitBorderEndWidth","webkitBorderHorizontalSpacing","webkitBorderImage","webkitBorderRadius","webkitBorderStart","webkitBorderStartColor","webkitBorderStartStyle","webkitBorderStartWidth","webkitBorderTopLeftRadius","webkitBorderTopRightRadius","webkitBorderVerticalSpacing","webkitBoxAlign","webkitBoxDecorationBreak","webkitBoxDirection","webkitBoxFlex","webkitBoxOrdinalGroup","webkitBoxOrient","webkitBoxPack","webkitBoxReflect","webkitBoxShadow","webkitBoxSizing","webkitClipPath","webkitColumnBreakAfter","webkitColumnBreakBefore","webkitColumnBreakInside","webkitColumnCount","webkitColumnGap","webkitColumnRule","webkitColumnRuleColor","webkitColumnRuleStyle","webkitColumnRuleWidth","webkitColumnSpan","webkitColumnWidth","webkitColumns","webkitFilter","webkitFlex","webkitFlexBasis","webkitFlexDirection","webkitFlexFlow","webkitFlexGrow","webkitFlexShrink","webkitFlexWrap","webkitFontFeatureSettings","webkitFontSmoothing","webkitHyphenateCharacter","webkitJustifyContent","webkitLineBreak","webkitLineClamp","webkitLocale","webkitLogicalHeight","webkitLogicalWidth","webkitMarginAfter","webkitMarginBefore","webkitMarginEnd","webkitMarginStart","webkitMask","webkitMaskBoxImage","webkitMaskBoxImageOutset","webkitMaskBoxImageRepeat","webkitMaskBoxImageSlice","webkitMaskBoxImageSource","webkitMaskBoxImageWidth","webkitMaskClip","webkitMaskComposite","webkitMaskImage","webkitMaskOrigin","webkitMaskPosition","webkitMaskPositionX","webkitMaskPositionY","webkitMaskRepeat","webkitMaskSize","webkitMaxLogicalHeight","webkitMaxLogicalWidth","webkitMinLogicalHeight","webkitMinLogicalWidth","webkitOpacity","webkitOrder","webkitPaddingAfter","webkitPaddingBefore","webkitPaddingEnd","webkitPaddingStart","webkitPerspective","webkitPerspectiveOrigin","webkitPerspectiveOriginX","webkitPerspectiveOriginY","webkitPrintColorAdjust","webkitRtlOrdering","webkitRubyPosition","webkitShapeImageThreshold","webkitShapeMargin","webkitShapeOutside","webkitTapHighlightColor","webkitTextCombine","webkitTextDecorationsInEffect","webkitTextEmphasis","webkitTextEmphasisColor","webkitTextEmphasisPosition","webkitTextEmphasisStyle","webkitTextFillColor","webkitTextOrientation","webkitTextSecurity","webkitTextSizeAdjust","webkitTextStroke","webkitTextStrokeColor","webkitTextStrokeWidth","webkitTransform","webkitTransformOrigin","webkitTransformOriginX","webkitTransformOriginY","webkitTransformOriginZ","webkitTransformStyle","webkitTransition","webkitTransitionDelay","webkitTransitionDuration","webkitTransitionProperty","webkitTransitionTimingFunction","webkitUserDrag","webkitUserModify","webkitUserSelect","webkitWritingMode","whiteSpace","whiteSpaceCollapse","widows","width","willChange","wordBreak","wordSpacing","wordWrap","writingMode","x","y","zIndex","zoom"];
    const cssIdlOwnNameSet = new Set(cssIdlOwnNames);
    const cssGhostOwnNames = new Set(["epubCaptionSide","epubTextCombine","epubTextEmphasis","epubTextEmphasisColor","epubTextEmphasisStyle","epubTextOrientation","epubTextTransform","epubWordBreak","epubWritingMode"]);
    const idlToCssName = (prop) => {
        let name = String(prop).replace(/[A-Z]/g, (m) => '-' + m.toLowerCase());
        if (name.startsWith('webkit-')) name = '-' + name;
        return name;
    };
    const createDeclaration = ({ getText, setText, parentRule = null, backing = null }) => {
        const target = Object.create(CSSStyleDeclaration.prototype);
        let proxy;
        proxy = new Proxy(target, {
            get(_target, prop, receiver) {
                if (prop === Symbol.iterator) {
                    return function* values() {
                        const entries = declarationEntries(proxy);
                        for (let i = 0; i < entries.length; i++) yield entries[i].name;
                    };
                }
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    return declarationEntries(proxy)[Number(prop)]?.name;
                }
                if (Reflect.has(CSSStyleDeclaration.prototype, prop)) {
                    return Reflect.get(CSSStyleDeclaration.prototype, prop, proxy);
                }
                if (backing && prop in backing) return backing[prop];
                if (typeof prop === 'string') {
                    const cssName = idlToCssName(prop);
                    return CSSStyleDeclaration.prototype.getPropertyValue.call(proxy, cssName);
                }
                return Reflect.get(target, prop, receiver);
            },
            set(_target, prop, value) {
                if (Reflect.has(CSSStyleDeclaration.prototype, prop)) {
                    const desc = Object.getOwnPropertyDescriptor(CSSStyleDeclaration.prototype, prop);
                    if (desc?.set) { desc.set.call(proxy, value); return true; }
                }
                if (typeof prop === 'string') {
                    const cssName = idlToCssName(prop);
                    CSSStyleDeclaration.prototype.setProperty.call(proxy, cssName, value, '');
                    return true;
                }
                return false;
            },
            has(_target, prop) {
                if (Reflect.has(CSSStyleDeclaration.prototype, prop)) return true;
                if (typeof prop === 'string' && /^\d+$/.test(prop)) return Number(prop) < declarationEntries(proxy).length;
                if (typeof prop === 'string' && cssIdlOwnNameSet.has(prop)) return true;
                if (backing && prop in backing) return true;
                return false;
            },
            ownKeys() {
                const entries = declarationEntries(proxy);
                return [
                    ...entries.map((_, index) => String(index)),
                    ...cssIdlOwnNames,
                ];
            },
            getOwnPropertyDescriptor(_target, prop) {
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    const name = declarationEntries(proxy)[Number(prop)]?.name;
                    if (name !== undefined) {
                        return { value: name, writable: false, enumerable: true, configurable: true };
                    }
                }
                if (typeof prop === 'string' && cssIdlOwnNameSet.has(prop)) {
                    if (cssGhostOwnNames.has(prop)) return undefined;
                    return {
                        value: CSSStyleDeclaration.prototype.getPropertyValue.call(proxy, idlToCssName(prop)),
                        writable: true,
                        enumerable: true,
                        configurable: true,
                    };
                }
                return undefined;
            },
        });
        declarationState.set(proxy, { getText, setText, parentRule, backing });
        return proxy;
    };

    // Preserve the DOM bootstrap's existing live style backend, but expose a
    // stable CSSStyleDeclaration wrapper with standard methods/descriptors.
    const elementStyleDescriptor = globalThis.Element?.prototype
        ? Object.getOwnPropertyDescriptor(globalThis.Element.prototype, 'style')
        : null;
    if (elementStyleDescriptor?.get) {
        Object.defineProperty(globalThis.Element.prototype, 'style', {
            get: mask(function style() {
                const backing = elementStyleDescriptor.get.call(this);
                if (backing && declarationCache.has(backing)) return declarationCache.get(backing);
                const view = createDeclaration({
                    getText: () => String(backing?.cssText || ''),
                    setText: (text) => {
                        if (backing) backing.cssText = text;
                        try { this.setAttribute('style', text); } catch (_) {}
                    },
                    backing,
                });
                if (backing) declarationCache.set(backing, view);
                return view;
            }, 'get style'),
            enumerable: true,
            configurable: true,
        });
    }

    globalThis.CSSStyleDeclaration = CSSStyleDeclaration;

    // ------------------------------------------------------------
    // CSSRule family
    // ------------------------------------------------------------
    const ruleState = new WeakMap();
    const ruleStateOf = (self) => {
        const state = ruleState.get(self);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const makeRuleCtor = (name, parentProto) => {
        const Ctor = illegal(name);
        Ctor.prototype = Object.create(parentProto || Object.prototype);
        Object.defineProperty(Ctor.prototype, 'constructor', {
            value: Ctor, writable: true, enumerable: false, configurable: true,
        });
        defineTag(Ctor.prototype, name);
        return Ctor;
    };

    const CSSRule = makeRuleCtor('CSSRule', Object.prototype);
    const ruleConstants = {
        STYLE_RULE: 1, CHARSET_RULE: 2, IMPORT_RULE: 3, MEDIA_RULE: 4,
        FONT_FACE_RULE: 5, PAGE_RULE: 6, KEYFRAMES_RULE: 7, KEYFRAME_RULE: 8,
        MARGIN_RULE: 9, NAMESPACE_RULE: 10, COUNTER_STYLE_RULE: 11,
        SUPPORTS_RULE: 12, FONT_FEATURE_VALUES_RULE: 14,
    };
    for (const [name, value] of Object.entries(ruleConstants)) {
        for (const owner of [CSSRule, CSSRule.prototype]) {
            Object.defineProperty(owner, name, {
                value, writable: false, enumerable: true, configurable: false,
            });
        }
    }
    defineGetter(CSSRule.prototype, 'type', function type() { return ruleStateOf(this).type; });
    defineGetter(CSSRule.prototype, 'cssText',
        function cssText() { return ruleStateOf(this).cssText(); },
        function cssText(value) { ruleStateOf(this).setCssText(String(value)); });
    defineGetter(CSSRule.prototype, 'parentRule', function parentRule() { return ruleStateOf(this).parentRule || null; });
    defineGetter(CSSRule.prototype, 'parentStyleSheet', function parentStyleSheet() { return ruleStateOf(this).sheet || null; });

    // Reparent the already-correct generic grouping/condition rule stubs so
    // their descendants inherit the real CSSRule base.
    if (globalThis.CSSGroupingRule?.prototype) {
        try { Object.setPrototypeOf(globalThis.CSSGroupingRule.prototype, CSSRule.prototype); } catch (_) {}
    }
    if (globalThis.CSSConditionRule?.prototype && globalThis.CSSGroupingRule?.prototype) {
        try { Object.setPrototypeOf(globalThis.CSSConditionRule.prototype, globalThis.CSSGroupingRule.prototype); } catch (_) {}
    }

    const CSSStyleRule = makeRuleCtor('CSSStyleRule', CSSRule.prototype);
    const mediaParent = globalThis.CSSConditionRule?.prototype || globalThis.CSSGroupingRule?.prototype || CSSRule.prototype;
    const CSSMediaRule = makeRuleCtor('CSSMediaRule', mediaParent);
    const CSSFontFaceRule = makeRuleCtor('CSSFontFaceRule', CSSRule.prototype);

    const makeRuleList = (getter) => new Proxy(Object.create(globalThis.CSSRuleList?.prototype || Object.prototype), {
        get(_target, prop) {
            const rules = getter();
            if (prop === 'length') return rules.length;
            if (prop === 'item') return mask(function item(index) { return rules[Number(index) >>> 0] || null; }, 'item');
            if (prop === Symbol.iterator) return mask(function* values() { yield* rules; }, 'values');
            if (prop === Symbol.toStringTag) return 'CSSRuleList';
            if (typeof prop === 'string' && /^\d+$/.test(prop)) return rules[Number(prop)];
            return undefined;
        },
        has(_target, prop) {
            if (prop === 'length' || prop === 'item') return true;
            return typeof prop === 'string' && /^\d+$/.test(prop) && Number(prop) < getter().length;
        },
        ownKeys() { return getter().map((_, i) => String(i)); },
        getOwnPropertyDescriptor(_target, prop) {
            if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                const value = getter()[Number(prop)];
                if (value !== undefined) return { value, writable: false, enumerable: true, configurable: true };
            }
            return undefined;
        },
    });

    const makeMediaList = (initial) => {
        let text = String(initial || '').trim();
        const target = Object.create(globalThis.MediaList?.prototype || Object.prototype);
        return new Proxy(target, {
            get(_target, prop) {
                const parts = text ? text.split(',').map((s) => s.trim()).filter(Boolean) : [];
                if (prop === 'mediaText') return text;
                if (prop === 'length') return parts.length;
                if (prop === 'item') return mask(function item(index) { return parts[Number(index) >>> 0] || null; }, 'item');
                if (prop === 'appendMedium') return mask(function appendMedium(value) {
                    const v = String(value).trim(); if (v && !parts.includes(v)) text = [...parts, v].join(', ');
                }, 'appendMedium');
                if (prop === 'deleteMedium') return mask(function deleteMedium(value) {
                    const v = String(value).trim(); text = parts.filter((part) => part !== v).join(', ');
                }, 'deleteMedium');
                if (prop === Symbol.toStringTag) return 'MediaList';
                if (typeof prop === 'string' && /^\d+$/.test(prop)) return parts[Number(prop)];
                return undefined;
            },
            set(_target, prop, value) {
                if (prop === 'mediaText') { text = String(value).trim(); return true; }
                return false;
            },
        });
    };

    if (globalThis.CSSGroupingRule?.prototype) {
        const groupingProto = globalThis.CSSGroupingRule.prototype;
        defineGetter(groupingProto, 'cssRules', function cssRules() {
            const state = ruleStateOf(this);
            if (!state.ruleList) state.ruleList = makeRuleList(() => state.nested || []);
            return state.ruleList;
        });
        defineMethod(groupingProto, 'insertRule', function insertRule(rule) {
            const state = ruleStateOf(this);
            const index = arguments.length > 1 ? Number(arguments[1]) >>> 0 : 0;
            const parsed = parseOneRule(String(rule), state.sheet, this);
            if (!parsed) throw new DOMException('Failed to parse the rule', 'SyntaxError');
            const slot = Math.min(index, state.nested.length);
            state.nested.splice(slot, 0, parsed);
            return slot;
        }, 1);
        defineMethod(groupingProto, 'deleteRule', function deleteRule(index) {
            const state = ruleStateOf(this);
            const i = Number(index) >>> 0;
            if (i >= state.nested.length) {
                throw new DOMException('Index or size is negative or greater than the allowed amount', 'IndexSizeError');
            }
            state.nested.splice(i, 1);
        }, 1);
    }
    if (globalThis.CSSConditionRule?.prototype) {
        defineGetter(globalThis.CSSConditionRule.prototype, 'conditionText', function conditionText() {
            const state = ruleStateOf(this);
            return state.media?.mediaText || state.conditionText || '';
        });
    }

    const reorderPrototype = (proto, names) => {
        const descriptors = new Map();
        for (const name of names) {
            const descriptor = Object.getOwnPropertyDescriptor(proto, name);
            if (descriptor) descriptors.set(name, descriptor);
        }
        for (const name of names) {
            try { delete proto[name]; } catch (_) {}
        }
        for (const name of names) {
            const descriptor = descriptors.get(name);
            if (descriptor) Object.defineProperty(proto, name, descriptor);
        }
    };
    if (globalThis.CSSGroupingRule?.prototype) {
        reorderPrototype(globalThis.CSSGroupingRule.prototype, ['cssRules', 'deleteRule', 'insertRule', 'constructor']);
    }
    if (globalThis.CSSConditionRule?.prototype) {
        reorderPrototype(globalThis.CSSConditionRule.prototype, ['conditionText', 'constructor']);
    }

    const makeStyleRule = (selector, body, sheet, parentRule = null) => {
        const rule = Object.create(CSSStyleRule.prototype);
        let selectorText = String(selector || '').trim();
        let declarationText = serializeDeclaration(parseDeclaration(body));
        const nested = [];
        const style = createDeclaration({
            getText: () => declarationText,
            setText: (text) => { declarationText = serializeDeclaration(parseDeclaration(text)); },
            parentRule: rule,
        });
        const state = {
            type: 1, sheet, parentRule, style, nested,
            cssText: () => `${selectorText} { ${style.cssText} }`,
            setCssText: (text) => {
                const match = /^\s*([^{}]+)\{([\s\S]*)\}\s*$/.exec(String(text));
                if (match) { selectorText = match[1].trim(); style.cssText = match[2]; }
            },
            get selectorText() { return selectorText; },
            set selectorText(value) { selectorText = String(value); },
        };
        ruleState.set(rule, state);
        return rule;
    };

    const makeFontFaceRule = (body, sheet, parentRule = null) => {
        const rule = Object.create(CSSFontFaceRule.prototype);
        let declarationText = serializeDeclaration(parseDeclaration(body));
        const style = createDeclaration({
            getText: () => declarationText,
            setText: (text) => { declarationText = serializeDeclaration(parseDeclaration(text)); },
            parentRule: rule,
        });
        ruleState.set(rule, {
            type: 5, sheet, parentRule, style,
            cssText: () => `@font-face { ${style.cssText} }`,
            setCssText: (text) => {
                const match = /@font-face\s*\{([\s\S]*)\}\s*$/i.exec(String(text));
                if (match) style.cssText = match[1];
            },
        });
        return rule;
    };

    const splitRules = (text) => {
        const source = String(text || '');
        const out = [];
        let start = 0, depth = 0, quote = '', escaped = false;
        for (let i = 0; i < source.length; i++) {
            const ch = source[i];
            if (quote) {
                if (escaped) escaped = false;
                else if (ch === '\\') escaped = true;
                else if (ch === quote) quote = '';
                continue;
            }
            if (ch === '"' || ch === "'") { quote = ch; continue; }
            if (ch === '{') depth++;
            else if (ch === '}') {
                depth--;
                if (depth === 0) {
                    const rule = source.slice(start, i + 1).trim();
                    if (rule) out.push(rule);
                    start = i + 1;
                }
            } else if (ch === ';' && depth === 0) {
                const rule = source.slice(start, i + 1).trim();
                if (rule) out.push(rule);
                start = i + 1;
            }
        }
        const tail = source.slice(start).trim();
        if (tail) out.push(tail);
        return out;
    };

    const parseOneRule = (text, sheet, parentRule = null) => {
        const source = String(text).trim();
        let match = /^@media\s+([^{}]+)\{([\s\S]*)\}\s*$/i.exec(source);
        if (match) {
            const rule = Object.create(CSSMediaRule.prototype);
            const media = makeMediaList(match[1].trim());
            const nested = [];
            const state = {
                type: 4, sheet, parentRule, media, nested,
                cssText: () => `@media ${media.mediaText} {\n${nested.map((r) => `  ${r.cssText}`).join('\n')}\n}`,
                setCssText: () => {},
            };
            ruleState.set(rule, state);
            nested.push(...splitRules(match[2]).map((part) => parseOneRule(part, sheet, rule)).filter(Boolean));
            return rule;
        }
        match = /^@font-face\s*\{([\s\S]*)\}\s*$/i.exec(source);
        if (match) return makeFontFaceRule(match[1], sheet, parentRule);
        match = /^([^{}]+)\{([\s\S]*)\}\s*$/.exec(source);
        if (match) return makeStyleRule(match[1], match[2], sheet, parentRule);
        return null;
    };

    defineGetter(CSSStyleRule.prototype, 'selectorText',
        function selectorText() { return ruleStateOf(this).selectorText; },
        function selectorText(value) { ruleStateOf(this).selectorText = value; });
    defineGetter(CSSStyleRule.prototype, 'style',
        function style() { return ruleStateOf(this).style; },
        function style(value) { ruleStateOf(this).style.cssText = value?.cssText ?? String(value ?? ''); });
    defineGetter(CSSStyleRule.prototype, 'styleMap', function styleMap() {
        return Object.create(globalThis.StylePropertyMap?.prototype || Object.prototype);
    });
    defineGetter(CSSStyleRule.prototype, 'cssRules', function cssRules() {
        const state = ruleStateOf(this);
        if (!state.ruleList) state.ruleList = makeRuleList(() => state.nested || []);
        return state.ruleList;
    });
    defineMethod(CSSStyleRule.prototype, 'insertRule', function insertRule(rule) {
        const state = ruleStateOf(this);
        const index = arguments.length > 1 ? Number(arguments[1]) >>> 0 : 0;
        const parsed = parseOneRule(String(rule), state.sheet, this);
        if (!parsed) throw new DOMException('Failed to parse the rule', 'SyntaxError');
        state.nested.splice(Math.min(index, state.nested.length), 0, parsed);
        return Math.min(index, state.nested.length - 1);
    }, 1);
    defineMethod(CSSStyleRule.prototype, 'deleteRule', function deleteRule(index) {
        ruleStateOf(this).nested.splice(Number(index) >>> 0, 1);
    }, 1);

    defineGetter(CSSMediaRule.prototype, 'media',
        function media() { return ruleStateOf(this).media; },
        function media(value) { ruleStateOf(this).media.mediaText = value?.mediaText ?? String(value ?? ''); });
    defineGetter(CSSFontFaceRule.prototype, 'style', function style() { return ruleStateOf(this).style; });

    globalThis.CSSRule = CSSRule;
    globalThis.CSSStyleRule = CSSStyleRule;
    globalThis.CSSMediaRule = CSSMediaRule;
    globalThis.CSSFontFaceRule = CSSFontFaceRule;

    // ------------------------------------------------------------
    // CSSStyleSheet
    // ------------------------------------------------------------
    const sheetState = new WeakMap();
    const sheetStateOf = (self) => {
        const state = sheetState.get(self);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const StyleSheetCtor = globalThis.StyleSheet;
    const styleSheetParentProto = StyleSheetCtor?.prototype || Object.prototype;

    function CSSStyleSheet() {
        if (!new.target) {
            throw new TypeError("Failed to construct 'CSSStyleSheet': Please use the 'new' operator, this DOM object constructor cannot be called as a function.");
        }
        const target = Object.create(CSSStyleSheet.prototype);
        let proxy;
        const state = {
            rules: [], roots: [], ownerRule: null, ruleList: null,
            replaceSync(text) {
                state.rules = splitRules(text).map((part) => parseOneRule(part, proxy, null)).filter(Boolean);
                state.sync();
            },
            sync() {
                const css = state.rules.map((rule) => rule.cssText).join('\n');
                for (const root of state.roots) {
                    try { root.textContent = css; } catch (_) {}
                }
            },
        };
        proxy = new Proxy(target, {
            get(_target, prop, receiver) {
                // Compatibility bridge for the existing DOM adoptedStyleSheets
                // implementation.  These names intentionally never appear in
                // ownKeys or on the public prototype.
                if (prop === '_roots') return state.roots;
                if (prop === '_replaceSync') return state.replaceSync.bind(state);
                if (prop === '_sync') return state.sync.bind(state);
                return Reflect.get(target, prop, receiver);
            },
            set(_target, prop, value, receiver) {
                if (prop === '_roots') { state.roots = Array.isArray(value) ? value : []; return true; }
                return Reflect.set(target, prop, value, receiver);
            },
        });
        sheetState.set(proxy, state);
        return proxy;
    }
    setLength(CSSStyleSheet, 0);
    mask(CSSStyleSheet, 'CSSStyleSheet');
    CSSStyleSheet.prototype = Object.create(styleSheetParentProto);
    Object.defineProperty(CSSStyleSheet.prototype, 'constructor', {
        value: CSSStyleSheet, writable: true, enumerable: false, configurable: true,
    });
    if (StyleSheetCtor) {
        try { Object.setPrototypeOf(CSSStyleSheet, StyleSheetCtor); } catch (_) {}
    }
    defineTag(CSSStyleSheet.prototype, 'CSSStyleSheet');
    defineGetter(CSSStyleSheet.prototype, 'cssRules', function cssRules() {
        const state = sheetStateOf(this);
        if (!state.ruleList) state.ruleList = makeRuleList(() => state.rules);
        return state.ruleList;
    });
    defineGetter(CSSStyleSheet.prototype, 'rules', function rules() { return this.cssRules; });
    defineGetter(CSSStyleSheet.prototype, 'ownerRule', function ownerRule() { return sheetStateOf(this).ownerRule; });
    defineMethod(CSSStyleSheet.prototype, 'insertRule', function insertRule(rule) {
        const state = sheetStateOf(this);
        const index = arguments.length > 1 ? Number(arguments[1]) >>> 0 : 0;
        if (index > state.rules.length) throw new DOMException('Index or size is negative or greater than the allowed amount', 'IndexSizeError');
        const parsed = parseOneRule(String(rule), this, null);
        if (!parsed) throw new DOMException('Failed to parse the rule', 'SyntaxError');
        state.rules.splice(index, 0, parsed);
        state.sync();
        return index;
    }, 1);
    defineMethod(CSSStyleSheet.prototype, 'deleteRule', function deleteRule(index) {
        const state = sheetStateOf(this);
        const i = Number(index) >>> 0;
        if (i >= state.rules.length) throw new DOMException('Index or size is negative or greater than the allowed amount', 'IndexSizeError');
        state.rules.splice(i, 1);
        state.sync();
    }, 1);
    defineMethod(CSSStyleSheet.prototype, 'replaceSync', function replaceSync(text) {
        sheetStateOf(this).replaceSync(String(text));
    }, 1);
    defineMethod(CSSStyleSheet.prototype, 'replace', function replace(text) {
        this.replaceSync(text);
        return Promise.resolve(this);
    }, 1);
    defineMethod(CSSStyleSheet.prototype, 'addRule', function addRule() {
        const selector = arguments.length > 0 ? String(arguments[0]) : 'undefined';
        const style = arguments.length > 1 ? String(arguments[1]) : 'undefined';
        const index = arguments.length > 2 ? Number(arguments[2]) >>> 0 : this.cssRules.length;
        this.insertRule(`${selector} { ${style} }`, Math.min(index, this.cssRules.length));
        return -1;
    }, 0);
    defineMethod(CSSStyleSheet.prototype, 'removeRule', function removeRule() {
        const index = arguments.length > 0 ? Number(arguments[0]) >>> 0 : 0;
        this.deleteRule(index);
    }, 0);

    globalThis.CSSStyleSheet = CSSStyleSheet;
})(globalThis);
