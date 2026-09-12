((globalThis) => {
    const ops = Deno.core.ops;
    const _getImageBytes = globalThis.__bo_get_image_bytes;
    try { delete globalThis.__bo_get_image_bytes; } catch (_) {}
    const _decodedImageIds = new WeakMap();
    const _imageBitmapState = new WeakMap();
    const _canvasGradientState = new WeakMap();
    const _canvasPatternState = new WeakMap();
    const _path2DState = new WeakMap();

    const _defineIdlMethod = (prototype, name, value) => {
        Object.defineProperty(prototype, name, {
            value,
            writable: true,
            enumerable: true,
            configurable: true,
        });
    };

    const _debugCanvas = (entry) => {
        if (!globalThis.__browser_oxide_debug) return;
        try {
            const log = globalThis.__oxCanvasDiag || (globalThis.__oxCanvasDiag = []);
            if (log.length < 240) log.push(entry);
        } catch (_) {}
    };

    const _decodeBytes = (bytes) => {
        if (!bytes || bytes.length === 0) return -1;
        const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
        let binary = '';
        const chunkSize = 0x8000;
        for (let offset = 0; offset < view.length; offset += chunkSize) {
            binary += String.fromCharCode(...view.subarray(offset, offset + chunkSize));
        }
        return ops.op_image_decode_base64(btoa(binary));
    };

    const _decodeImageSource = (source) => {
        if (!source || typeof source !== 'object' || typeof _getImageBytes !== 'function') return -1;
        const cached = _decodedImageIds.get(source);
        if (cached !== undefined) return cached;
        const bytes = _getImageBytes(source);
        if (!bytes || bytes.length === 0) return -1;
        const imageId = _decodeBytes(bytes);
        if (imageId >= 0) _decodedImageIds.set(source, imageId);
        return imageId;
    };

    // Replace the early shared-API placeholders with a functional
    // ImageBitmap implementation. Cloudflare creates an ImageBitmap from
    // its `/ci/` image, then reads the result back through a canvas; a zero
    // sized placeholder turns that pixel challenge into a 0x0 sample.
    function ImageBitmap() {
        throw new TypeError("Failed to construct 'ImageBitmap': Illegal constructor");
    }
    delete ImageBitmap.prototype.constructor;
    Object.defineProperty(ImageBitmap.prototype, 'width', {
        get() { return (_imageBitmapState.get(this) || {}).width || 0; },
        enumerable: true, configurable: true,
    });
    Object.defineProperty(ImageBitmap.prototype, 'height', {
        get() { return (_imageBitmapState.get(this) || {}).height || 0; },
        enumerable: true, configurable: true,
    });
    Object.defineProperty(ImageBitmap.prototype, 'close', {
        value: function close() { _imageBitmapState.delete(this); },
        writable: true, enumerable: true, configurable: true,
    });
    Object.defineProperty(ImageBitmap.prototype, 'constructor', {
        value: ImageBitmap, writable: true, configurable: true,
    });
    Object.defineProperty(ImageBitmap.prototype, Symbol.toStringTag, {
        value: 'ImageBitmap', configurable: true,
    });
    const _makeImageBitmap = (state) => {
        const bitmap = Object.create(ImageBitmap.prototype);
        _imageBitmapState.set(bitmap, state);
        return bitmap;
    };
    globalThis.ImageBitmap = ImageBitmap;
    // DOM canvas backing state must not leak as page-visible `_canvasId` own
    // properties. Keep the native canvas id in a WeakMap; the legacy fallback
    // remains only for internal standalone canvas objects created inside this
    // bootstrap and is never used by document.createElement.
    const _canvasElementIds = new WeakMap();
    const _canvasBackingId = value => {
        if (!value || (typeof value !== 'object' && typeof value !== 'function')) return undefined;
        const hidden = _canvasElementIds.get(value);
        return hidden !== undefined ? hidden : value._canvasId;
    };
    const _readImageBitmapEnum = (options, property, values, typeName, defaultValue) => {
        const raw = options[property];
        if (raw === undefined) return defaultValue;
        const value = String(raw);
        if (!values.includes(value)) {
            throw new TypeError(
                "Failed to execute 'createImageBitmap' on 'Window': Failed to read the '" +
                property + "' property from 'ImageBitmapOptions': The provided value '" + value +
                "' is not a valid enum value of type " + typeName + "."
            );
        }
        return value;
    };
    const _readImageBitmapUnsignedLong = (options, property) => {
        const raw = options[property];
        if (raw === undefined) return undefined;
        const value = Number(raw);
        const prefix = "Failed to execute 'createImageBitmap' on 'Window': Failed to read the '" +
            property + "' property from 'ImageBitmapOptions': ";
        if (Number.isNaN(value)) {
            throw new TypeError(prefix + "Value is not of type 'unsigned long'.");
        }
        if (!Number.isFinite(value)) {
            throw new TypeError(prefix + "Value is infinite and not of type 'unsigned long'.");
        }
        if (value < 0 || value > 0xFFFFFFFF) {
            throw new TypeError(prefix + "Value is outside the 'unsigned long' value range.");
        }
        return Math.trunc(value);
    };
    const _readImageBitmapOptions = rawOptions => {
        if (rawOptions == null) rawOptions = {};
        if ((typeof rawOptions !== 'object' && typeof rawOptions !== 'function')) {
            throw new TypeError(
                "Failed to execute 'createImageBitmap' on 'Window': " +
                "The provided value is not of type 'ImageBitmapOptions'."
            );
        }
        // Web IDL dictionary members are observed in lexicographic order.
        const colorSpaceConversion = _readImageBitmapEnum(
            rawOptions, 'colorSpaceConversion', ['none', 'default'],
            'ColorSpaceConversion', 'default'
        );
        const imageOrientation = _readImageBitmapEnum(
            rawOptions, 'imageOrientation', ['from-image', 'flipY'],
            'ImageOrientation', 'from-image'
        );
        const premultiplyAlpha = _readImageBitmapEnum(
            rawOptions, 'premultiplyAlpha', ['none', 'premultiply', 'default'],
            'PremultiplyAlpha', 'default'
        );
        const resizeHeight = _readImageBitmapUnsignedLong(rawOptions, 'resizeHeight');
        const resizeQuality = _readImageBitmapEnum(
            rawOptions, 'resizeQuality', ['pixelated', 'low', 'medium', 'high'],
            'ResizeQuality', 'low'
        );
        const resizeWidth = _readImageBitmapUnsignedLong(rawOptions, 'resizeWidth');
        return {
            colorSpaceConversion,
            imageOrientation,
            premultiplyAlpha,
            resizeHeight,
            resizeQuality,
            resizeWidth,
        };
    };
    globalThis.createImageBitmap = function createImageBitmap(image) {
        const sourceSnapshot = {
            tag: Object.prototype.toString.call(image),
            width: Number(image && image.width) || 0,
            height: Number(image && image.height) || 0,
            argc: arguments.length,
        };
        const args = arguments;
        const finish = () => {
            let sourceKind = '';
            let sourceId = -1;
            let sourceBytes = new Uint8Array(0);
            let sourceWidth = 0;
            let sourceHeight = 0;

            const bitmapState = image && _imageBitmapState.get(image);
            if (bitmapState) {
                sourceWidth = Number(bitmapState.width) || 0;
                sourceHeight = Number(bitmapState.height) || 0;
                if (bitmapState.imageId !== undefined) {
                    sourceKind = 'image';
                    sourceId = bitmapState.imageId;
                } else if (bitmapState.canvasId !== undefined) {
                    sourceKind = 'canvas';
                    sourceId = bitmapState.canvasId;
                }
            } else {
                const imageCanvasId = _canvasBackingId(image);
                if (imageCanvasId !== undefined) {
                    sourceKind = 'canvas';
                    sourceId = imageCanvasId;
                    sourceWidth = Number(image.width) || 0;
                    sourceHeight = Number(image.height) || 0;
                } else if (image instanceof ImageData) {
                    sourceKind = 'rgba';
                    sourceWidth = Number(image.width) || 0;
                    sourceHeight = Number(image.height) || 0;
                    if (image.data instanceof Uint8ClampedArray || image.data instanceof Uint8Array) {
                        sourceBytes = new Uint8Array(
                            image.data.buffer,
                            image.data.byteOffset,
                            image.data.byteLength,
                        );
                    } else {
                        sourceBytes = Uint8Array.from(image.data, value =>
                            Math.max(0, Math.min(255, Math.round(Number(value) * 255)))
                        );
                    }
                } else {
                    const bytes = (typeof _getImageBytes === 'function' && _getImageBytes(image))
                        || (image && image._data)
                        || null;
                    sourceId = _decodeBytes(bytes);
                    if (sourceId >= 0) {
                        sourceKind = 'image';
                        const dimensions = ops.op_image_get_dimensions(sourceId);
                        sourceWidth = Number(image && image.naturalWidth)
                            || Number(dimensions[0]) || 0;
                        sourceHeight = Number(image && image.naturalHeight)
                            || Number(dimensions[1]) || 0;
                    }
                }
            }

            if (!sourceKind || sourceWidth <= 0 || sourceHeight <= 0) {
                throw new DOMException('The source image could not be decoded.', 'InvalidStateError');
            }

            const hasCrop = args.length >= 5;
            let sx = 0;
            let sy = 0;
            let sw = sourceWidth;
            let sh = sourceHeight;
            const options = _readImageBitmapOptions(hasCrop ? args[5] : args[1]);

            if (hasCrop) {
                sx = Math.trunc(Number(args[1]));
                sy = Math.trunc(Number(args[2]));
                sw = Math.trunc(Number(args[3]));
                sh = Math.trunc(Number(args[4]));
                if (sw === 0) {
                    throw new RangeError("Failed to execute 'createImageBitmap' on 'Window': The crop rect width is 0.");
                }
                if (sh === 0) {
                    throw new RangeError("Failed to execute 'createImageBitmap' on 'Window': The crop rect height is 0.");
                }
                // Negative crop extents grow in the opposite direction; they
                // do not mirror the selected pixels.
                if (sw < 0) { sx += sw; sw = -sw; }
                if (sh < 0) { sy += sh; sh = -sh; }
            }

            const resizeWidth = options.resizeWidth;
            const resizeHeight = options.resizeHeight;
            if (resizeWidth === 0 || resizeHeight === 0) {
                throw new DOMException(
                    "Failed to execute 'createImageBitmap' on 'Window': The options's resizeWidth or resizeHeight is 0.",
                    'InvalidStateError'
                );
            }

            const outputWidth = resizeWidth !== undefined
                ? resizeWidth
                : resizeHeight !== undefined
                    ? Math.max(1, Math.ceil(sw * resizeHeight / sh))
                    : sw;
            const outputHeight = resizeHeight !== undefined
                ? resizeHeight
                : resizeWidth !== undefined
                    ? Math.max(1, Math.ceil(sh * resizeWidth / sw))
                    : sh;
            const resizeQuality = options.resizeQuality;
            const flipY = options.imageOrientation === 'flipY';

            const transformedId = ops.op_image_bitmap_transform(
                sourceKind,
                sourceId,
                sourceBytes,
                sourceWidth,
                sourceHeight,
                sx,
                sy,
                sw,
                sh,
                outputWidth,
                outputHeight,
                resizeQuality,
                flipY,
            );
            if (transformedId < 0) {
                throw new DOMException('The source image could not be decoded.', 'InvalidStateError');
            }
            const bitmap = _makeImageBitmap({
                imageId: transformedId,
                width: outputWidth,
                height: outputHeight,
            });
            _debugCanvas({
                op: 'createImageBitmap', source: sourceSnapshot,
                width: bitmap.width, height: bitmap.height,
            });
            return bitmap;
        };
        if (image && image.complete === false && typeof image.decode === 'function') {
            return image.decode().then(finish);
        }
        try { return Promise.resolve(finish()); }
        catch (error) { return Promise.reject(error); }
    };
    if (typeof _maskAsNative === 'function') {
        _maskAsNative(ImageBitmap.prototype, 'width', 'height', 'close');
        if (typeof _maskFunction === 'function') {
            _maskFunction(ImageBitmap, 'ImageBitmap');
            _maskFunction(globalThis.createImageBitmap, 'createImageBitmap');
        }
    } else if (typeof _maskFunction === 'function') {
        _maskFunction(ImageBitmap, 'ImageBitmap');
        _maskFunction(ImageBitmap.prototype.close, 'close');
        _maskFunction(globalThis.createImageBitmap, 'createImageBitmap');
    }

    // Canvas helper objects are WebIDL objects in Chromium, not ordinary
    // JavaScript bags. Keep implementation state out of own-property
    // enumeration and expose the same prototype method shape as Blink.
    function CanvasGradient() {
        throw new TypeError("Failed to construct 'CanvasGradient': Illegal constructor");
    }
    _defineIdlMethod(CanvasGradient.prototype, 'addColorStop', function addColorStop(offset, color) {
        const state = _canvasGradientState.get(this);
        if (!state) throw new TypeError('Illegal invocation');
        state.stops.push({ offset: Number(offset), color: String(color) });
    });
    Object.defineProperty(CanvasGradient.prototype, Symbol.toStringTag, {
        value: 'CanvasGradient', configurable: true,
    });
    const _makeCanvasGradient = (state) => {
        const gradient = Object.create(CanvasGradient.prototype);
        _canvasGradientState.set(gradient, state);
        return gradient;
    };

    function CanvasPattern() {
        throw new TypeError("Failed to construct 'CanvasPattern': Illegal constructor");
    }
    _defineIdlMethod(CanvasPattern.prototype, 'setTransform', function setTransform() {
        const state = _canvasPatternState.get(this);
        if (!state) throw new TypeError('Illegal invocation');
        state.transform = arguments.length ? arguments[0] : null;
    });
    Object.defineProperty(CanvasPattern.prototype, Symbol.toStringTag, {
        value: 'CanvasPattern', configurable: true,
    });
    const _makeCanvasPattern = (state) => {
        const pattern = Object.create(CanvasPattern.prototype);
        _canvasPatternState.set(pattern, state);
        return pattern;
    };

    function Path2D() {
        if (!new.target) {
            throw new TypeError("Failed to construct 'Path2D': Please use the 'new' operator");
        }
        const source = arguments[0];
        let commands = [];
        if (source && _path2DState.has(source)) {
            commands = _path2DState.get(source).commands.slice();
        } else if (source !== undefined) {
            commands.push(['svgPath', String(source)]);
        }
        _path2DState.set(this, { commands });
    }
    const _pathCommand = (name, arity, normalize) => {
        let fn;
        switch (arity) {
            case 0: fn = function () { normalize(this, arguments); }; break;
            case 1: fn = function (a) { normalize(this, arguments); }; break;
            case 2: fn = function (a, b) { normalize(this, arguments); }; break;
            case 4: fn = function (a, b, c, d) { normalize(this, arguments); }; break;
            case 5: fn = function (a, b, c, d, e) { normalize(this, arguments); }; break;
            case 6: fn = function (a, b, c, d, e, f) { normalize(this, arguments); }; break;
            case 7: fn = function (a, b, c, d, e, f, g) { normalize(this, arguments); }; break;
            default: fn = function () { normalize(this, arguments); };
        }
        Object.defineProperty(fn, 'name', { value: name, configurable: true });
        return fn;
    };
    const _recordPathCommand = (name) => (self, args) => {
        const state = _path2DState.get(self);
        if (!state) throw new TypeError('Illegal invocation');
        state.commands.push([name, ...Array.from(args)]);
    };
    const _pathMethods = {
        addPath: 1,
        arc: 5,
        arcTo: 5,
        bezierCurveTo: 6,
        closePath: 0,
        ellipse: 7,
        lineTo: 2,
        moveTo: 2,
        quadraticCurveTo: 4,
        rect: 4,
        roundRect: 4,
    };
    for (const [name, arity] of Object.entries(_pathMethods)) {
        _defineIdlMethod(
            Path2D.prototype,
            name,
            _pathCommand(name, arity, _recordPathCommand(name)),
        );
    }
    Object.defineProperty(Path2D.prototype, Symbol.toStringTag, {
        value: 'Path2D', configurable: true,
    });

    globalThis.CanvasGradient = CanvasGradient;
    globalThis.CanvasPattern = CanvasPattern;
    globalThis.Path2D = Path2D;
    if (typeof _maskFunction === 'function') {
        _maskFunction(CanvasGradient, 'CanvasGradient');
        _maskFunction(CanvasPattern, 'CanvasPattern');
        _maskFunction(Path2D, 'Path2D');
        _maskFunction(CanvasGradient.prototype.addColorStop, 'addColorStop');
        _maskFunction(CanvasPattern.prototype.setTransform, 'setTransform');
        for (const name of Object.keys(_pathMethods)) {
            _maskFunction(Path2D.prototype[name], name);
        }
    }

    // -- Canvas-based font detection support -----------------------------
    // Some scripts detect installed fonts
    // by comparing measureText widths across candidate families: if
    // measureText("...", "Arial") differs from measureText("...", "sans-serif")
    // the family is reported as installed. Our font_database.rs aliases
    // every Chrome-on-OS family to bundled Liberation Sans/Serif/Mono,
    // so without this shim every probe collapses to identical widths and
    // the sensor reports `fonts=null`. Inject a deterministic, sub-pixel
    // family-derived delta so distinct family names produce distinct
    // widths — exactly what real Chrome does naturally because each face
    // ships with its own metrics.
    const _fontProbeFnvHash = (str) => {
        let h = 2166136261 >>> 0;
        for (let i = 0; i < str.length; i++) {
            h ^= str.charCodeAt(i);
            h = (h + ((h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24))) >>> 0;
        }
        return h;
    };
    // Mirror the fonts present on Chrome for each OS — keep in sync with
    // `window_bootstrap.js` `Font enumeration spoofing` block.
    const _FONT_LIST_BY_OS = {
        "Windows": new Set([
            "arial","arial black","calibri","cambria","comic sans ms","consolas",
            "courier new","georgia","impact","lucida console","segoe ui","tahoma",
            "times new roman","trebuchet ms","verdana",
        ]),
        "macOS": new Set([
            "arial","arial black","courier new","georgia","helvetica",
            "helvetica neue","lucida grande","menlo","monaco","sf pro",
            "times new roman","trebuchet ms","verdana",
        ]),
        "Linux": new Set([
            "arial","courier new","dejavu sans","dejavu sans mono","dejavu serif",
            "liberation mono","liberation sans","liberation serif","noto sans",
            "times new roman","ubuntu","verdana",
        ]),
    };
    const _resolveInstalledFonts = () => {
        const os = _getOsName();
        return _FONT_LIST_BY_OS[os] || _FONT_LIST_BY_OS["Linux"];
    };
    const _getOsName = () => {
        try {
            const has = ops.op_has_stealth_profile && ops.op_has_stealth_profile();
            return has ? (ops.op_get_profile_value("os_name") || "Linux") : "Linux";
        } catch (_e) {
            return "Linux";
        }
    };
    let _canvasSeedCache = null;
    const _getCanvasSeed = () => {
        if (_canvasSeedCache !== null) return _canvasSeedCache;
        try {
            const has = ops.op_has_stealth_profile && ops.op_has_stealth_profile();
            const raw = has ? ops.op_get_profile_value("canvas_seed") : "0";
            _canvasSeedCache = BigInt(raw || "0");
        } catch (_e) {
            _canvasSeedCache = 0n;
        }
        return _canvasSeedCache;
    };
    const _GENERIC_FAMILIES = new Set(["sans-serif","serif","monospace","cursive","fantasy","system-ui","ui-sans-serif","ui-serif","ui-monospace"]);
    const _primaryFontFamily = (fontStr) => {
        if (!fontStr) return null;
        // Strip CSS font shorthand prefix (style/variant/weight/stretch/size/line-height).
        // The family list is everything after the last whitespace following the size token.
        const sizeMatch = fontStr.match(/(\d+(?:\.\d+)?)(px|pt|em|rem|%|vh|vw)\s+(.+)$/);
        const familyList = sizeMatch ? sizeMatch[3] : fontStr;
        const first = familyList.split(",")[0] || "";
        return first.replace(/["']/g, "").trim().toLowerCase();
    };
    // 0.0 .. ~3.5 px deterministic delta. Sub-character-width so layout
    // stays stable, large enough to clear 1e-3 fingerprint comparisons.
    const _fontFamilyWidthDelta = (family) => {
        if (!family) return 0;
        if (_GENERIC_FAMILIES.has(family)) return 0; // generics are baselines
        if (!_resolveInstalledFonts().has(family)) return 0; // not installed on this OS
        const h = _fontProbeFnvHash(family);
        return (h % 7000) / 2000; // 0.0 .. 3.5 px
    };

    // Parse CSS color to [r, g, b, a]
    function _parseColor(str) {
        const named = { red:[255,0,0,255], green:[0,128,0,255], blue:[0,0,255,255],
            black:[0,0,0,255], white:[255,255,255,255], yellow:[255,255,0,255],
            cyan:[0,255,255,255], magenta:[255,0,255,255], transparent:[0,0,0,0] };
        if (named[str]) return named[str];
        if (str.startsWith('#')) {
            const h = str.slice(1);
            if (h.length === 3) return [parseInt(h[0]+h[0],16), parseInt(h[1]+h[1],16), parseInt(h[2]+h[2],16), 255];
            if (h.length === 6) return [parseInt(h.slice(0,2),16), parseInt(h.slice(2,4),16), parseInt(h.slice(4,6),16), 255];
        }
        const m = str.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
        if (m) return [+m[1], +m[2], +m[3], m[4] !== undefined ? Math.round(+m[4]*255) : 255];
        return [0, 0, 0, 255];
    }

    function _parseDisplayP3(str) {
        const match = String(str).trim().match(
            /^color\(display-p3\s+([-+.\deE]+)\s+([-+.\deE]+)\s+([-+.\deE]+)(?:\s*\/\s*([-+.\deE]+))?\)$/
        );
        if (!match) return null;
        const p3 = [+match[1], +match[2], +match[3]];
        const alpha = match[4] === undefined ? 1 : +match[4];
        if (![...p3, alpha].every(Number.isFinite)) return null;
        const linearize = value => value <= 0.04045
            ? value / 12.92
            : Math.pow((value + 0.055) / 1.055, 2.4);
        const encode = value => value <= 0.0031308
            ? 12.92 * value
            : 1.055 * Math.pow(value, 1 / 2.4) - 0.055;
        const p = p3.map(linearize);
        const xyz = [
            0.4865709486482162*p[0] + 0.26566769316909306*p[1] + 0.1982172852343625*p[2],
            0.2289745640697488*p[0] + 0.6917385218365064*p[1] + 0.079286914093745*p[2],
            0.04511338185890264*p[1] + 1.043944368900976*p[2],
        ];
        const srgb = [
            encode(3.2409699419045226*xyz[0] - 1.537383177570094*xyz[1] - 0.4986107602930034*xyz[2]),
            encode(-0.9692436362808796*xyz[0] + 1.8759675015077202*xyz[1] + 0.04155505740717559*xyz[2]),
            encode(0.05563007969699366*xyz[0] - 0.20397695888897652*xyz[1] + 1.0569715142428786*xyz[2]),
        ];
        return { p3, srgb, alpha };
    }

    const _imageDataState = new WeakMap();
    class ImageData {
        constructor(data, width) {
            let pixels;
            let actualWidth;
            let actualHeight;
            let settings;
            if (typeof data === 'number') {
                // constructor(width, height, settings?)
                actualWidth = Number(data) >>> 0;
                actualHeight = Number(width) >>> 0;
                settings = arguments[2];
                pixels = new Uint8ClampedArray(actualWidth * actualHeight * 4);
            } else {
                // constructor(data, width, height?, settings?)
                pixels = data;
                actualWidth = Number(width) >>> 0;
                actualHeight = arguments[2] === undefined
                    ? (actualWidth ? Math.floor(pixels.length / 4 / actualWidth) : 0)
                    : Number(arguments[2]) >>> 0;
                settings = arguments[3];
            }
            const state = {
                data: pixels,
                width: actualWidth,
                height: actualHeight,
                colorSpace: settings && settings.colorSpace === 'display-p3'
                    ? 'display-p3' : 'srgb',
                pixelFormat: settings && settings.pixelFormat === 'rgba-float16'
                    ? 'rgba-float16' : 'rgba-unorm8',
            };
            _imageDataState.set(this, state);
            // Blink materializes `data` as the sole own property while also
            // keeping the WebIDL getter on the prototype.
            Object.defineProperty(this, 'data', {
                value: pixels,
                writable: false,
                enumerable: true,
                configurable: true,
            });
        }
        get data() {
            const state = _imageDataState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.data;
        }
        get width() {
            const state = _imageDataState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.width;
        }
        get height() {
            const state = _imageDataState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.height;
        }
        get colorSpace() {
            const state = _imageDataState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.colorSpace;
        }
        get pixelFormat() {
            const state = _imageDataState.get(this);
            if (!state) throw new TypeError('Illegal invocation');
            return state.pixelFormat;
        }
    }
    for (const name of ['data', 'width', 'height', 'colorSpace', 'pixelFormat']) {
        const descriptor = Object.getOwnPropertyDescriptor(ImageData.prototype, name);
        descriptor.enumerable = true;
        Object.defineProperty(ImageData.prototype, name, descriptor);
    }
    Object.defineProperty(ImageData.prototype, Symbol.toStringTag, {
        value: 'ImageData', configurable: true,
    });
    globalThis.ImageData = ImageData;
    _maskFunction(ImageData, 'ImageData');
    if (typeof _maskAsNative === 'function') {
        _maskAsNative(ImageData.prototype, 'data', 'width', 'height', 'colorSpace', 'pixelFormat');
    }

    class CanvasRenderingContext2D {
        #id;
        constructor(id) { this.#id = id; }

        // Style
        set fillStyle(v) {
            _debugCanvas({ op: 'setFillStyle', value: String(v) });
            this._wideGamutFill = typeof v === 'string' ? _parseDisplayP3(v) : null;
            const gradientState = v && typeof v === 'object' ? _canvasGradientState.get(v) : null;
            if (gradientState) {
                const stops = gradientState.stops.map(s => {
                    const c = _parseColor(s.color);
                    return [s.offset, c[0], c[1], c[2], c[3]];
                });
                let coords;
                if (gradientState.type === "linear") {
                    coords = [gradientState.x0, gradientState.y0, gradientState.x1, gradientState.y1];
                } else {
                    coords = [
                        gradientState.x0, gradientState.y0, gradientState.r0,
                        gradientState.x1, gradientState.y1, gradientState.r1,
                    ];
                }
                ops.op_canvas_set_fill_gradient(
                    this.#id, gradientState.type, JSON.stringify({ coords, stops })
                );
            } else {
                ops.op_canvas_set_fill_style(this.#id, String(v));
            }
        }
        set strokeStyle(v) { _debugCanvas({ op: 'setStrokeStyle', value: String(v) }); ops.op_canvas_set_stroke_style(this.#id, String(v)); }
        set lineWidth(v) { ops.op_canvas_set_line_width(this.#id, +v); }
        set globalAlpha(v) { _debugCanvas({ op: 'setGlobalAlpha', value: +v }); ops.op_canvas_set_global_alpha(this.#id, +v); }
        set font(v) { this._font = String(v); ops.op_canvas_set_font(this.#id, this._font); }
        get font() { return this._font || "10px sans-serif"; }

        // Rectangles
        fillRect(x, y, w, h) {
            _debugCanvas({ op: 'fillRect', x, y, w, h });
            ops.op_canvas_fill_rect(this.#id, x, y, w, h);
            this._wideGamutRect = this._wideGamutFill
                ? { x: +x, y: +y, w: +w, h: +h, color: this._wideGamutFill }
                : null;
        }
        strokeRect(x, y, w, h) { ops.op_canvas_stroke_rect(this.#id, x, y, w, h); }
        clearRect(x, y, w, h) { _debugCanvas({ op: 'clearRect', x, y, w, h }); ops.op_canvas_clear_rect(this.#id, x, y, w, h); }

        // Path
        beginPath() { _debugCanvas({ op: 'beginPath' }); ops.op_canvas_begin_path(this.#id); }
        moveTo(x, y) { _debugCanvas({ op: 'moveTo', x, y }); ops.op_canvas_move_to(this.#id, x, y); }
        lineTo(x, y) { _debugCanvas({ op: 'lineTo', x, y }); ops.op_canvas_line_to(this.#id, x, y); }
        fill(fillRule) {
            const evenOdd = fillRule === 'evenodd';
            _debugCanvas({ op: 'fill', fillRule: evenOdd ? 'evenodd' : 'nonzero' });
            ops.op_canvas_fill(this.#id, evenOdd);
        }
        stroke() { ops.op_canvas_stroke(this.#id); }
        closePath() { _debugCanvas({ op: 'closePath' }); ops.op_canvas_close_path(this.#id); }
        arc(x, y, r, startAngle, endAngle, counterclockwise) {
            _debugCanvas({ op: 'arc', x, y, r, startAngle, endAngle, counterclockwise: !!counterclockwise });
            ops.op_canvas_arc(this.#id, x, y, r, startAngle, endAngle, !!counterclockwise);
        }
        arcTo(x1, y1, x2, y2, r) {
            _debugCanvas({ op: 'arcTo', x1, y1, x2, y2, r });
            ops.op_canvas_arc_to(this.#id, x1, y1, x2, y2, r);
        }
        bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y) {
            _debugCanvas({ op: 'bezierCurveTo', cp1x, cp1y, cp2x, cp2y, x, y });
            ops.op_canvas_bezier_curve_to(this.#id, cp1x, cp1y, cp2x, cp2y, x, y);
        }
        quadraticCurveTo(cpx, cpy, x, y) {
            _debugCanvas({ op: 'quadraticCurveTo', cpx, cpy, x, y });
            ops.op_canvas_quadratic_curve_to(this.#id, cpx, cpy, x, y);
        }
        ellipse(x, y, rx, ry, rotation, startAngle, endAngle, counterclockwise) {
            _debugCanvas({ op: 'ellipse', x, y, rx, ry, rotation, startAngle, endAngle, counterclockwise: !!counterclockwise });
            ops.op_canvas_ellipse(this.#id, x, y, rx, ry, rotation, startAngle, endAngle, !!counterclockwise);
        }
        rect(x, y, w, h) { this.moveTo(x,y); this.lineTo(x+w,y); this.lineTo(x+w,y+h); this.lineTo(x,y+h); this.closePath(); }

        // Text
        fillText(text, x, y) {
            _debugCanvas({ op: 'fillText', text: String(text).slice(0, 80), x, y, font: this.font });
            ops.op_canvas_fill_text(this.#id, text, x, y);
        }
        strokeText(text, x, y) { ops.op_canvas_stroke_text(this.#id, text, x, y); }
        measureText(text) {
            // Full 13-field TextMetrics shaped in Rust (T1.2 font stack).
            // actualBoundingBox* come from the real glyph run, not a
            // derived ratio — this is what fingerprint sites probe.
            const m = ops.op_canvas_measure_text_full(this.#id, text);
            // Per-family micro-delta so canvas-based font detection works.
            // See `_fontFamilyWidthDelta` for rationale.
            const fam = _primaryFontFamily(this._font);
            const deltaPerChar = _fontFamilyWidthDelta(fam);
            const len = (typeof text === "string") ? text.length : 0;
            const widthDelta = deltaPerChar * Math.max(1, len) * 0.25;
            return {
                width: m.width + widthDelta,
                actualBoundingBoxLeft: m.actual_bounding_box_left,
                actualBoundingBoxRight: m.actual_bounding_box_right + widthDelta,
                actualBoundingBoxAscent: m.actual_bounding_box_ascent,
                actualBoundingBoxDescent: m.actual_bounding_box_descent,
                fontBoundingBoxAscent: m.font_bounding_box_ascent,
                fontBoundingBoxDescent: m.font_bounding_box_descent,
                emHeightAscent: m.em_height_ascent,
                emHeightDescent: m.em_height_descent,
                alphabeticBaseline: m.alphabetic_baseline,
                hangingBaseline: m.hanging_baseline,
                ideographicBaseline: m.ideographic_baseline,
            };
        }

        // Transform
        save() { ops.op_canvas_save(this.#id); }
        restore() { ops.op_canvas_restore(this.#id); }
        translate(x, y) { ops.op_canvas_translate(this.#id, x, y); }
        rotate(angle) { ops.op_canvas_rotate(this.#id, angle); }
        scale(x, y) { ops.op_canvas_scale(this.#id, x, y); }
        setTransform(a, b, c, d, e, f) {
            // Spec also accepts a single DOMMatrix-init dict; handle both shapes.
            if (typeof a === "object" && a !== null) {
                ops.op_canvas_set_transform(
                    this.#id, a.a ?? 1, a.b ?? 0, a.c ?? 0, a.d ?? 1, a.e ?? 0, a.f ?? 0
                );
            } else {
                ops.op_canvas_set_transform(this.#id, a, b, c, d, e, f);
            }
        }
        resetTransform() { ops.op_canvas_reset_transform(this.#id); }
        getTransform() { return {a:1,b:0,c:0,d:1,e:0,f:0}; }

        // Image data — real pixel ops
        getImageData(x, y, w, h, settings) {
            let raw = ops.op_canvas_get_image_data(this.#id, x, y, w, h);
            const colorSpace = settings && settings.colorSpace === 'display-p3'
                ? 'display-p3' : 'srgb';
            const pixelFormat = settings && settings.pixelFormat === 'rgba-float16'
                ? 'rgba-float16' : 'rgba-unorm8';
            const tracked = this._wideGamutRect;
            const covers = tracked && x >= tracked.x && y >= tracked.y
                && x + w <= tracked.x + tracked.w
                && y + h <= tracked.y + tracked.h;
            if (covers) {
                const source = colorSpace === 'display-p3'
                    ? tracked.color.p3 : tracked.color.srgb;
                const values = [...source, tracked.color.alpha];
                if (pixelFormat === 'rgba-float16' && typeof Float16Array === 'function') {
                    raw = new Float16Array(w * h * 4);
                    for (let i = 0; i < raw.length; i++) raw[i] = values[i % 4];
                } else if (colorSpace === 'display-p3') {
                    const quantized = values.map(value => Math.max(0, Math.min(255,
                        Math.floor(value * 255 + 0.499999))));
                    raw = new Uint8ClampedArray(w * h * 4);
                    for (let i = 0; i < raw.length; i++) raw[i] = quantized[i % 4];
                }
            } else if (pixelFormat === 'rgba-float16' && typeof Float16Array === 'function') {
                const floats = new Float16Array(raw.length);
                for (let i = 0; i < raw.length; i++) floats[i] = raw[i] / 255;
                raw = floats;
            }
            if (globalThis.__browser_oxide_debug) {
                let hash = 2166136261 >>> 0;
                let nonzero = 0;
                for (let i = 0; i < raw.length; i++) {
                    const value = raw[i];
                    if (value) nonzero++;
                    hash ^= value;
                    hash = Math.imul(hash, 16777619) >>> 0;
                }
                _debugCanvas({
                    op: 'getImageData', x, y, w, h, hash, nonzero,
                    head: Array.from(raw.slice(0, 16)),
                });
            }
            const data = pixelFormat === 'rgba-float16' && raw instanceof Float16Array
                ? raw : new Uint8ClampedArray(raw);
            return new ImageData(data, w, h, { colorSpace, pixelFormat });
        }
        putImageData(imageData, dx, dy) {
            _debugCanvas({
                op: 'putImageData', dx, dy,
                width: imageData && imageData.width,
                height: imageData && imageData.height,
                head: imageData && imageData.data
                    ? Array.from(imageData.data.slice(0, 16))
                    : [],
            });
            ops.op_canvas_put_image_data(this.#id, imageData.data, dx, dy, imageData.width, imageData.height);
        }
        createImageData(w, h) { return new ImageData(w, h); }
        drawImage(source, dx, dy) {
            const bitmapState = source && _imageBitmapState.get(source);
            if (bitmapState) {
                if (bitmapState.canvasId !== undefined) {
                    ops.op_canvas_draw_image(this.#id, bitmapState.canvasId, dx || 0, dy || 0);
                } else if (bitmapState.imageId !== undefined) {
                    ops.op_canvas_draw_decoded_image(
                        this.#id, bitmapState.imageId, dx || 0, dy || 0
                    );
                }
                _debugCanvas({
                    op: 'drawImageBitmap', argc: arguments.length,
                    dx: dx || 0, dy: dy || 0,
                    width: bitmapState.width, height: bitmapState.height,
                });
                return;
            }
            // source can be another canvas element — get its internal ID
            const sourceCanvasId = _canvasBackingId(source);
            if (sourceCanvasId !== undefined) {
                _debugCanvas({
                    op: 'drawCanvas', argc: arguments.length,
                    dx: dx || 0, dy: dy || 0,
                    width: Number(source.width) || 0,
                    height: Number(source.height) || 0,
                });
                ops.op_canvas_draw_image(this.#id, sourceCanvasId, dx || 0, dy || 0);
                return;
            }
            // HTMLImageElement pixels are retained privately by the DOM
            // image loader. Decode once per element and composite them onto
            // the backing canvas, matching the common 3-argument overload.
            const imageId = _decodeImageSource(source);
            _debugCanvas({
                op: 'drawImage', argc: arguments.length,
                imageId, dx: dx || 0, dy: dy || 0,
                width: source && source.naturalWidth,
                height: source && source.naturalHeight,
            });
            if (imageId >= 0) {
                ops.op_canvas_draw_decoded_image(this.#id, imageId, dx || 0, dy || 0);
            }
        }

        // Gradient — JS-side objects that track color stops
        createLinearGradient(x0, y0, x1, y1) {
            return _makeCanvasGradient({
                stops: [], type: 'linear', x0, y0, x1, y1,
            });
        }
        createRadialGradient(x0, y0, r0, x1, y1, r1) {
            return _makeCanvasGradient({
                stops: [], type: 'radial', x0, y0, r0, x1, y1, r1,
            });
        }
        createPattern(image, repetition) {
            return _makeCanvasPattern({
                image,
                repetition: repetition === undefined ? 'repeat' : String(repetition),
                transform: null,
            });
        }

        // Clip
        clip() {}
        isPointInPath() { return false; }
        isPointInStroke() { return false; }
    }

    // WebGL extension objects are branded WebIDL objects, not ordinary `{}`
    // bags. Chromium exposes one stable object per extension/context, with
    // constants and methods on a shared extension-specific prototype. Keep
    // the cache and brand state in WeakMaps so calling getExtension() does not
    // add implementation fields to the WebGL context or extension object.
    //
    // The table below is the stable WebGL2 extension surface observed in both
    // Chrome 148/macOS and Chromium 154/macOS. Constant values are WebGL
    // registry values; method arities/descriptors were captured from Chromium.
    const _webglExtensionCache = new WeakMap();
    const _webglExtensionBrand = new WeakMap();
    const _webglExtensionPrototypes = new Map();
    const _webglExtensionSpecs = {
        'ANGLE_instanced_arrays': ['ANGLEInstancedArrays', {
            VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE: 35070,
        }, {
            drawArraysInstancedANGLE: 4, drawElementsInstancedANGLE: 5,
            vertexAttribDivisorANGLE: 2,
        }],
        'EXT_blend_minmax': ['EXTBlendMinMax', { MIN_EXT: 32775, MAX_EXT: 32776 }, {}],
        'EXT_clip_control': ['EXTClipControl', {
            LOWER_LEFT_EXT: 36001, UPPER_LEFT_EXT: 36002,
            NEGATIVE_ONE_TO_ONE_EXT: 37726, ZERO_TO_ONE_EXT: 37727,
            CLIP_ORIGIN_EXT: 37724, CLIP_DEPTH_MODE_EXT: 37725,
        }, { clipControlEXT: 2 }],
        'EXT_color_buffer_float': ['EXTColorBufferFloat', {}, {}],
        'EXT_color_buffer_half_float': ['EXTColorBufferHalfFloat', {
            RGBA16F_EXT: 34842, RGB16F_EXT: 34843,
            FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT: 33297,
            UNSIGNED_NORMALIZED_EXT: 35863,
        }, {}],
        'EXT_conservative_depth': ['EXTConservativeDepth', {}, {}],
        'EXT_depth_clamp': ['EXTDepthClamp', { DEPTH_CLAMP_EXT: 34383 }, {}],
        'EXT_disjoint_timer_query': ['EXTDisjointTimerQuery', {
            QUERY_COUNTER_BITS_EXT: 34916, CURRENT_QUERY_EXT: 34917,
            QUERY_RESULT_EXT: 34918, QUERY_RESULT_AVAILABLE_EXT: 34919,
            TIME_ELAPSED_EXT: 35007, TIMESTAMP_EXT: 36392, GPU_DISJOINT_EXT: 36795,
        }, {
            beginQueryEXT: 2, createQueryEXT: 0, deleteQueryEXT: 1,
            endQueryEXT: 1, getQueryEXT: 2, getQueryObjectEXT: 2,
            isQueryEXT: 1, queryCounterEXT: 2,
        }],
        'EXT_disjoint_timer_query_webgl2': ['EXTDisjointTimerQueryWebGL2', {
            QUERY_COUNTER_BITS_EXT: 34916, TIME_ELAPSED_EXT: 35007,
            TIMESTAMP_EXT: 36392, GPU_DISJOINT_EXT: 36795,
        }, { queryCounterEXT: 2 }],
        'EXT_float_blend': ['EXTFloatBlend', {}, {}],
        'EXT_frag_depth': ['EXTFragDepth', {}, {}],
        'EXT_polygon_offset_clamp': ['EXTPolygonOffsetClamp', {
            POLYGON_OFFSET_CLAMP_EXT: 36379,
        }, { polygonOffsetClampEXT: 3 }],
        'EXT_render_snorm': ['EXTRenderSnorm', {}, {}],
        'EXT_sRGB': ['EXTsRGB', {
            SRGB_EXT: 35904, SRGB_ALPHA_EXT: 35906, SRGB8_ALPHA8_EXT: 35907,
            FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT: 33296,
        }, {}],
        'EXT_shader_texture_lod': ['EXTShaderTextureLOD', {}, {}],
        'EXT_texture_compression_bptc': ['EXTTextureCompressionBPTC', {
            COMPRESSED_RGBA_BPTC_UNORM_EXT: 36492,
            COMPRESSED_SRGB_ALPHA_BPTC_UNORM_EXT: 36493,
            COMPRESSED_RGB_BPTC_SIGNED_FLOAT_EXT: 36494,
            COMPRESSED_RGB_BPTC_UNSIGNED_FLOAT_EXT: 36495,
        }, {}],
        'EXT_texture_compression_rgtc': ['EXTTextureCompressionRGTC', {
            COMPRESSED_RED_RGTC1_EXT: 36283,
            COMPRESSED_SIGNED_RED_RGTC1_EXT: 36284,
            COMPRESSED_RED_GREEN_RGTC2_EXT: 36285,
            COMPRESSED_SIGNED_RED_GREEN_RGTC2_EXT: 36286,
        }, {}],
        'EXT_texture_filter_anisotropic': ['EXTTextureFilterAnisotropic', {
            TEXTURE_MAX_ANISOTROPY_EXT: 34046,
            MAX_TEXTURE_MAX_ANISOTROPY_EXT: 34047,
        }, {}],
        'EXT_texture_mirror_clamp_to_edge': ['EXTTextureMirrorClampToEdge', {
            MIRROR_CLAMP_TO_EDGE_EXT: 34627,
        }, {}],
        'EXT_texture_norm16': ['EXTTextureNorm16', {
            R16_EXT: 33322, RG16_EXT: 33324, RGB16_EXT: 32852, RGBA16_EXT: 32859,
            R16_SNORM_EXT: 36760, RG16_SNORM_EXT: 36761,
            RGB16_SNORM_EXT: 36762, RGBA16_SNORM_EXT: 36763,
        }, {}],
        'KHR_parallel_shader_compile': ['KHRParallelShaderCompile', {
            COMPLETION_STATUS_KHR: 37297,
        }, {}],
        'NV_shader_noperspective_interpolation': ['NVShaderNoperspectiveInterpolation', {}, {}],
        'OES_draw_buffers_indexed': ['OESDrawBuffersIndexed', {}, {
            blendEquationSeparateiOES: 3, blendEquationiOES: 2,
            blendFuncSeparateiOES: 5, blendFunciOES: 3, colorMaskiOES: 5,
            disableiOES: 2, enableiOES: 2,
        }],
        'OES_element_index_uint': ['OESElementIndexUint', {}, {}],
        'OES_fbo_render_mipmap': ['OESFboRenderMipmap', {}, {}],
        'OES_sample_variables': ['OESSampleVariables', {}, {}],
        'OES_shader_multisample_interpolation': ['OESShaderMultisampleInterpolation', {
            MIN_FRAGMENT_INTERPOLATION_OFFSET_OES: 36443,
            MAX_FRAGMENT_INTERPOLATION_OFFSET_OES: 36444,
            FRAGMENT_INTERPOLATION_OFFSET_BITS_OES: 36445,
        }, {}],
        'OES_standard_derivatives': ['OESStandardDerivatives', {
            FRAGMENT_SHADER_DERIVATIVE_HINT_OES: 35723,
        }, {}],
        'OES_texture_float': ['OESTextureFloat', {}, {}],
        'OES_texture_float_linear': ['OESTextureFloatLinear', {}, {}],
        'OES_texture_half_float': ['OESTextureHalfFloat', { HALF_FLOAT_OES: 36193 }, {}],
        'OES_texture_half_float_linear': ['OESTextureHalfFloatLinear', {}, {}],
        'OES_vertex_array_object': ['OESVertexArrayObject', {
            VERTEX_ARRAY_BINDING_OES: 34229,
        }, {
            bindVertexArrayOES: 0, createVertexArrayOES: 0,
            deleteVertexArrayOES: 0, isVertexArrayOES: 0,
        }],
        'WEBGL_blend_func_extended': ['WebGLBlendFuncExtended', {
            SRC1_COLOR_WEBGL: 35065, SRC1_ALPHA_WEBGL: 34185,
            ONE_MINUS_SRC1_COLOR_WEBGL: 35066, ONE_MINUS_SRC1_ALPHA_WEBGL: 35067,
            MAX_DUAL_SOURCE_DRAW_BUFFERS_WEBGL: 35068,
        }, {}],
        'WEBGL_clip_cull_distance': ['WebGLClipCullDistance', {
            MAX_CLIP_DISTANCES_WEBGL: 3378, MAX_CULL_DISTANCES_WEBGL: 33529,
            MAX_COMBINED_CLIP_AND_CULL_DISTANCES_WEBGL: 33530,
            CLIP_DISTANCE0_WEBGL: 12288, CLIP_DISTANCE1_WEBGL: 12289,
            CLIP_DISTANCE2_WEBGL: 12290, CLIP_DISTANCE3_WEBGL: 12291,
            CLIP_DISTANCE4_WEBGL: 12292, CLIP_DISTANCE5_WEBGL: 12293,
            CLIP_DISTANCE6_WEBGL: 12294, CLIP_DISTANCE7_WEBGL: 12295,
        }, {}],
        'WEBGL_color_buffer_float': ['WebGLColorBufferFloat', {
            RGBA32F_EXT: 34836, FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT: 33297,
            UNSIGNED_NORMALIZED_EXT: 35863,
        }, {}],
        'WEBGL_compressed_texture_astc': ['WebGLCompressedTextureASTC', {
            COMPRESSED_RGBA_ASTC_4x4_KHR: 37808, COMPRESSED_RGBA_ASTC_5x4_KHR: 37809,
            COMPRESSED_RGBA_ASTC_5x5_KHR: 37810, COMPRESSED_RGBA_ASTC_6x5_KHR: 37811,
            COMPRESSED_RGBA_ASTC_6x6_KHR: 37812, COMPRESSED_RGBA_ASTC_8x5_KHR: 37813,
            COMPRESSED_RGBA_ASTC_8x6_KHR: 37814, COMPRESSED_RGBA_ASTC_8x8_KHR: 37815,
            COMPRESSED_RGBA_ASTC_10x5_KHR: 37816, COMPRESSED_RGBA_ASTC_10x6_KHR: 37817,
            COMPRESSED_RGBA_ASTC_10x8_KHR: 37818, COMPRESSED_RGBA_ASTC_10x10_KHR: 37819,
            COMPRESSED_RGBA_ASTC_12x10_KHR: 37820, COMPRESSED_RGBA_ASTC_12x12_KHR: 37821,
            COMPRESSED_SRGB8_ALPHA8_ASTC_4x4_KHR: 37840,
            COMPRESSED_SRGB8_ALPHA8_ASTC_5x4_KHR: 37841,
            COMPRESSED_SRGB8_ALPHA8_ASTC_5x5_KHR: 37842,
            COMPRESSED_SRGB8_ALPHA8_ASTC_6x5_KHR: 37843,
            COMPRESSED_SRGB8_ALPHA8_ASTC_6x6_KHR: 37844,
            COMPRESSED_SRGB8_ALPHA8_ASTC_8x5_KHR: 37845,
            COMPRESSED_SRGB8_ALPHA8_ASTC_8x6_KHR: 37846,
            COMPRESSED_SRGB8_ALPHA8_ASTC_8x8_KHR: 37847,
            COMPRESSED_SRGB8_ALPHA8_ASTC_10x5_KHR: 37848,
            COMPRESSED_SRGB8_ALPHA8_ASTC_10x6_KHR: 37849,
            COMPRESSED_SRGB8_ALPHA8_ASTC_10x8_KHR: 37850,
            COMPRESSED_SRGB8_ALPHA8_ASTC_10x10_KHR: 37851,
            COMPRESSED_SRGB8_ALPHA8_ASTC_12x10_KHR: 37852,
            COMPRESSED_SRGB8_ALPHA8_ASTC_12x12_KHR: 37853,
        }, { getSupportedProfiles: 0 }],
        'WEBGL_compressed_texture_etc': ['WebGLCompressedTextureETC', {
            COMPRESSED_R11_EAC: 37488, COMPRESSED_SIGNED_R11_EAC: 37489,
            COMPRESSED_RG11_EAC: 37490, COMPRESSED_SIGNED_RG11_EAC: 37491,
            COMPRESSED_RGB8_ETC2: 37492, COMPRESSED_SRGB8_ETC2: 37493,
            COMPRESSED_RGB8_PUNCHTHROUGH_ALPHA1_ETC2: 37494,
            COMPRESSED_SRGB8_PUNCHTHROUGH_ALPHA1_ETC2: 37495,
            COMPRESSED_RGBA8_ETC2_EAC: 37496, COMPRESSED_SRGB8_ALPHA8_ETC2_EAC: 37497,
        }, {}],
        'WEBGL_compressed_texture_etc1': ['WebGLCompressedTextureETC1', {
            COMPRESSED_RGB_ETC1_WEBGL: 36196,
        }, {}],
        'WEBGL_compressed_texture_pvrtc': ['WebGLCompressedTexturePVRTC', {
            COMPRESSED_RGB_PVRTC_4BPPV1_IMG: 35840, COMPRESSED_RGB_PVRTC_2BPPV1_IMG: 35841,
            COMPRESSED_RGBA_PVRTC_4BPPV1_IMG: 35842, COMPRESSED_RGBA_PVRTC_2BPPV1_IMG: 35843,
        }, {}],
        'WEBGL_compressed_texture_s3tc': ['WebGLCompressedTextureS3TC', {
            COMPRESSED_RGB_S3TC_DXT1_EXT: 33776, COMPRESSED_RGBA_S3TC_DXT1_EXT: 33777,
            COMPRESSED_RGBA_S3TC_DXT3_EXT: 33778, COMPRESSED_RGBA_S3TC_DXT5_EXT: 33779,
        }, {}],
        'WEBGL_compressed_texture_s3tc_srgb': ['WebGLCompressedTextureS3TCsRGB', {
            COMPRESSED_SRGB_S3TC_DXT1_EXT: 35916,
            COMPRESSED_SRGB_ALPHA_S3TC_DXT1_EXT: 35917,
            COMPRESSED_SRGB_ALPHA_S3TC_DXT3_EXT: 35918,
            COMPRESSED_SRGB_ALPHA_S3TC_DXT5_EXT: 35919,
        }, {}],
        'WEBGL_debug_renderer_info': ['WebGLDebugRendererInfo', {
            UNMASKED_VENDOR_WEBGL: 37445, UNMASKED_RENDERER_WEBGL: 37446,
        }, {}],
        'WEBGL_debug_shaders': ['WebGLDebugShaders', {}, { getTranslatedShaderSource: 1 }],
        'WEBGL_depth_texture': ['WebGLDepthTexture', { UNSIGNED_INT_24_8_WEBGL: 34042 }, {}],
        'WEBGL_draw_buffers': ['WebGLDrawBuffers', {
            COLOR_ATTACHMENT0_WEBGL: 36064, COLOR_ATTACHMENT1_WEBGL: 36065,
            COLOR_ATTACHMENT2_WEBGL: 36066, COLOR_ATTACHMENT3_WEBGL: 36067,
            COLOR_ATTACHMENT4_WEBGL: 36068, COLOR_ATTACHMENT5_WEBGL: 36069,
            COLOR_ATTACHMENT6_WEBGL: 36070, COLOR_ATTACHMENT7_WEBGL: 36071,
            COLOR_ATTACHMENT8_WEBGL: 36072, COLOR_ATTACHMENT9_WEBGL: 36073,
            COLOR_ATTACHMENT10_WEBGL: 36074, COLOR_ATTACHMENT11_WEBGL: 36075,
            COLOR_ATTACHMENT12_WEBGL: 36076, COLOR_ATTACHMENT13_WEBGL: 36077,
            COLOR_ATTACHMENT14_WEBGL: 36078, COLOR_ATTACHMENT15_WEBGL: 36079,
            DRAW_BUFFER0_WEBGL: 34853, DRAW_BUFFER1_WEBGL: 34854,
            DRAW_BUFFER2_WEBGL: 34855, DRAW_BUFFER3_WEBGL: 34856,
            DRAW_BUFFER4_WEBGL: 34857, DRAW_BUFFER5_WEBGL: 34858,
            DRAW_BUFFER6_WEBGL: 34859, DRAW_BUFFER7_WEBGL: 34860,
            DRAW_BUFFER8_WEBGL: 34861, DRAW_BUFFER9_WEBGL: 34862,
            DRAW_BUFFER10_WEBGL: 34863, DRAW_BUFFER11_WEBGL: 34864,
            DRAW_BUFFER12_WEBGL: 34865, DRAW_BUFFER13_WEBGL: 34866,
            DRAW_BUFFER14_WEBGL: 34867, DRAW_BUFFER15_WEBGL: 34868,
            MAX_COLOR_ATTACHMENTS_WEBGL: 36063, MAX_DRAW_BUFFERS_WEBGL: 34852,
        }, { drawBuffersWEBGL: 1 }],
        'WEBGL_lose_context': ['WebGLLoseContext', {}, { loseContext: 0, restoreContext: 0 }],
        'WEBGL_multi_draw': ['WebGLMultiDraw', {}, {
            multiDrawArraysInstancedWEBGL: 8, multiDrawArraysWEBGL: 6,
            multiDrawElementsInstancedWEBGL: 9, multiDrawElementsWEBGL: 7,
        }],
        'WEBGL_polygon_mode': ['WebGLPolygonMode', {
            POLYGON_MODE_WEBGL: 2880, POLYGON_OFFSET_LINE_WEBGL: 10754,
            LINE_WEBGL: 6913, FILL_WEBGL: 6914,
        }, { polygonModeWEBGL: 2 }],
        'WEBGL_provoking_vertex': ['WebGLProvokingVertex', {
            FIRST_VERTEX_CONVENTION_WEBGL: 36429,
            LAST_VERTEX_CONVENTION_WEBGL: 36430,
            PROVOKING_VERTEX_WEBGL: 36431,
        }, { provokingVertexWEBGL: 1 }],
        'WEBGL_render_shared_exponent': ['WebGLRenderSharedExponent', {}, {}],
        'WEBGL_stencil_texturing': ['WebGLStencilTexturing', {
            DEPTH_STENCIL_TEXTURE_MODE_WEBGL: 37098, STENCIL_INDEX_WEBGL: 6401,
        }, {}],
    };

    const _webglTimerQueryPrototype = Object.create(Object.prototype);
    Object.defineProperty(_webglTimerQueryPrototype, Symbol.toStringTag, {
        value: 'WebGLTimerQueryEXT', writable: false, enumerable: false, configurable: true,
    });
    const _webglVertexArrayObjectOESPrototype = Object.create(Object.prototype);
    Object.defineProperty(_webglVertexArrayObjectOESPrototype, Symbol.toStringTag, {
        value: 'WebGLVertexArrayObjectOES', writable: false, enumerable: false, configurable: true,
    });

    const _makeWebGLExtensionMethod = (tag, name, length) => {
        const method = function (..._args) {
            if (_webglExtensionBrand.get(this) !== tag) throw new TypeError('Illegal invocation');
            if (name === 'getSupportedProfiles') return ['ldr', 'hdr'];
            if (name === 'getTranslatedShaderSource') return '';
            if (name === 'createQueryEXT') return Object.create(_webglTimerQueryPrototype);
            if (name === 'isQueryEXT') return false;
            if (name === 'getQueryEXT') return null;
            if (name === 'getQueryObjectEXT') {
                if (_args[1] === 34919) return false; // QUERY_RESULT_AVAILABLE_EXT
                if (_args[1] === 34918) return 0; // QUERY_RESULT_EXT
                return null;
            }
            if (name === 'createVertexArrayOES') return Object.create(_webglVertexArrayObjectOESPrototype);
            if (name === 'isVertexArrayOES') return false;
            return undefined;
        };
        Object.defineProperty(method, 'name', { value: name, configurable: true });
        Object.defineProperty(method, 'length', { value: length, configurable: true });
        try { if (typeof _maskFunction === 'function') _maskFunction(method, name); } catch (_) {}
        return method;
    };

    const _getWebGLExtensionPrototype = (canonicalName) => {
        let prototype = _webglExtensionPrototypes.get(canonicalName);
        if (prototype) return prototype;
        const spec = _webglExtensionSpecs[canonicalName];
        if (!spec) return null;
        const [tag, constants, methods] = spec;
        prototype = Object.create(Object.prototype);
        for (const [name, value] of Object.entries(constants)) {
            Object.defineProperty(prototype, name, {
                value, writable: false, enumerable: true, configurable: false,
            });
        }
        for (const [name, length] of Object.entries(methods)) {
            Object.defineProperty(prototype, name, {
                value: _makeWebGLExtensionMethod(tag, name, length),
                writable: true, enumerable: true, configurable: true,
            });
        }
        Object.defineProperty(prototype, Symbol.toStringTag, {
            value: tag, writable: false, enumerable: false, configurable: true,
        });
        _webglExtensionPrototypes.set(canonicalName, prototype);
        return prototype;
    };

    const _getWebGLExtensionObject = (context, requestedName) => {
        const requested = String(requestedName).toLowerCase();
        const supported = context.getSupportedExtensions();
        const canonical = supported.find(name => name.toLowerCase() === requested);
        if (!canonical) return null;

        let cache = _webglExtensionCache.get(context);
        if (!cache) {
            cache = new Map();
            _webglExtensionCache.set(context, cache);
        }
        if (cache.has(canonical)) return cache.get(canonical);

        const spec = _webglExtensionSpecs[canonical];
        if (!spec) {
            const fallback = {};
            cache.set(canonical, fallback);
            return fallback;
        }
        const extension = Object.create(_getWebGLExtensionPrototype(canonical));
        _webglExtensionBrand.set(extension, spec[0]);
        cache.set(canonical, extension);
        return extension;
    };

    // Build the shared prototypes while the bootstrap-only native masking
    // helpers are still present. Extension *instances* remain lazy/per-context,
    // but their methods must already carry the native Function#toString tag by
    // the time cleanup removes `_maskFunction` from the page-visible global.
    for (const extensionName of Object.keys(_webglExtensionSpecs)) {
        _getWebGLExtensionPrototype(extensionName);
    }

    // WebGL object state is host-managed. Real WebGLShader/WebGLProgram/
    // WebGLUniformLocation wrappers expose no implementation fields of their
    // own, so keep every bit of lifecycle state out-of-band.
    const _webglShaderState = new WeakMap();
    const _webglProgramState = new WeakMap();
    const _webglUniformLocationState = new WeakMap();
    const _webglBufferState = new WeakMap();
    const _webglBufferBindings = new WeakMap();
    const _webglTextureState = new WeakMap();
    const _webglTextureBindings = new WeakMap();
    const _webglFramebufferState = new WeakMap();
    const _webglFramebufferBinding = new WeakMap();
    const _webglFramebufferObjects = new WeakMap();
    const _webglRenderbufferState = new WeakMap();
    const _webglRenderbufferBinding = new WeakMap();
    const _webglVertexArrayState = new WeakMap();
    const _webglVertexArrayBinding = new WeakMap();
    const _webglQueryState = new WeakMap();
    const _webglQueryBindings = new WeakMap();
    const _webglSamplerState = new WeakMap();
    const _webglSamplerBindings = new WeakMap();
    const _webglTransformFeedbackState = new WeakMap();
    const _webglTransformFeedbackBinding = new WeakMap();
    const _webglSyncState = new WeakMap();
    const _webglContextState = new WeakMap();
    const _webglContextErrors = new WeakMap();
    const _webglCurrentProgram = new WeakMap();

    function _webglState(ctx) {
        return _webglContextState.get(ctx) || null;
    }

    function _webglBindings(ctx) {
        let bindings = _webglBufferBindings.get(ctx);
        if (!bindings) {
            bindings = { array: null, element: null };
            _webglBufferBindings.set(ctx, bindings);
        }
        return bindings;
    }

    function _webglTextureBindingState(ctx) {
        let state = _webglTextureBindings.get(ctx);
        if (!state) {
            state = {
                activeUnit: 0,
                units: Array.from({ length: 32 }, () => ({ twoD: null, cube: null })),
            };
            _webglTextureBindings.set(ctx, state);
        }
        return state;
    }

    function _webglTextureSlot(target) {
        if (target === WebGLRenderingContext.TEXTURE_2D) return 'twoD';
        if (target === WebGLRenderingContext.TEXTURE_CUBE_MAP) return 'cube';
        return null;
    }

    function _webglTextureImageBindingSlot(target) {
        if (target === WebGLRenderingContext.TEXTURE_2D) return 'twoD';
        if (target >= WebGLRenderingContext.TEXTURE_CUBE_MAP_POSITIVE_X &&
            target <= WebGLRenderingContext.TEXTURE_CUBE_MAP_NEGATIVE_Z) return 'cube';
        return null;
    }

    function _webglBoundTexture(ctx, target, imageTarget = false) {
        const slot = imageTarget ? _webglTextureImageBindingSlot(target) : _webglTextureSlot(target);
        if (!slot) return undefined;
        const state = _webglTextureBindingState(ctx);
        return state.units[state.activeUnit][slot];
    }

    function _webglFramebuffers(ctx) {
        let set = _webglFramebufferObjects.get(ctx);
        if (!set) {
            set = new Set();
            _webglFramebufferObjects.set(ctx, set);
        }
        return set;
    }

    function _webglDetachObjectFromFramebuffers(ctx, object) {
        for (const framebuffer of _webglFramebuffers(ctx)) {
            const state = _webglFramebufferState.get(framebuffer);
            if (!state || state.deleted) continue;
            for (const [attachment, entry] of state.attachments) {
                if (entry && entry.object === object) state.attachments.delete(attachment);
            }
        }
    }

    function _webglAttachmentAllowed(attachment) {
        return attachment === WebGLRenderingContext.COLOR_ATTACHMENT0 ||
            attachment === WebGLRenderingContext.DEPTH_ATTACHMENT ||
            attachment === WebGLRenderingContext.STENCIL_ATTACHMENT ||
            attachment === WebGLRenderingContext.DEPTH_STENCIL_ATTACHMENT;
    }

    function _configureWebGLContext(ctx, options = {}) {
        let state = _webglContextState.get(ctx);
        if (!state) {
            const width = options.width === undefined ? 300 : Number(options.width);
            const height = options.height === undefined ? 150 : Number(options.height);
            state = {
                canvasId: options.canvasId,
                width: Number.isFinite(width) ? width : 300,
                height: Number.isFinite(height) ? height : 150,
                clearColor: [0, 0, 0, 0],
                viewport: [0, 0, Number.isFinite(width) ? width : 300, Number.isFinite(height) ? height : 150],
                canvas: options.canvas || null,
                isWebGL2: !!options.isWebGL2,
            };
            _webglContextState.set(ctx, state);
        } else {
            if (options.canvasId !== undefined) state.canvasId = options.canvasId;
            if (options.width !== undefined && Number.isFinite(Number(options.width))) state.width = Number(options.width);
            if (options.height !== undefined && Number.isFinite(Number(options.height))) state.height = Number(options.height);
            if (options.canvas !== undefined) state.canvas = options.canvas;
            if (options.isWebGL2 !== undefined) state.isWebGL2 = !!options.isWebGL2;
            if (options.resetViewport) state.viewport = [0, 0, state.width, state.height];
        }
        return ctx;
    }

    function _webglDrawingWidth(ctx) {
        const state = _webglState(ctx);
        if (!state) return 0;
        const value = state.canvas && Number(state.canvas.width);
        return Number.isFinite(value) ? value : state.width;
    }

    function _webglDrawingHeight(ctx) {
        const state = _webglState(ctx);
        if (!state) return 0;
        const value = state.canvas && Number(state.canvas.height);
        return Number.isFinite(value) ? value : state.height;
    }

    function _setWebGLError(ctx, code) {
        // WebGL records the first outstanding error until getError() consumes
        // it. Later errors do not replace an earlier unconsumed one.
        if ((_webglContextErrors.get(ctx) || 0) === 0) {
            _webglContextErrors.set(ctx, code >>> 0);
        }
    }

    function _newWebGLObject(name, stateMap, state) {
        const Ctor = globalThis[name];
        const proto = Ctor && Ctor.prototype ? Ctor.prototype : Object.prototype;
        const object = Object.create(proto);
        stateMap.set(object, state);
        return object;
    }

    function _requireWebGLObject(name, stateMap, value, nullable = false, operation = 'WebGL operation') {
        if (nullable && value == null) return null;
        const Ctor = globalThis[name];
        if (!value || !Ctor || !Ctor.prototype || !Ctor.prototype.isPrototypeOf(value)) {
            throw new TypeError(`Failed to execute '${operation}' on 'WebGLRenderingContext': parameter 1 is not of type '${name}'.`);
        }
        return stateMap.get(value) || null;
    }

    function _requireWebGL2Object(name, stateMap, value, nullable = false, operation = 'WebGL2 operation') {
        if (nullable && value == null) return null;
        const Ctor = globalThis[name];
        if (!value || !Ctor || !Ctor.prototype || !Ctor.prototype.isPrototypeOf(value)) {
            throw new TypeError(`Failed to execute '${operation}' on 'WebGL2RenderingContext': parameter 1 is not of type '${name}'.`);
        }
        return stateMap.get(value) || null;
    }

    function _webgl2QueryBindingsFor(ctx) {
        let bindings = _webglQueryBindings.get(ctx);
        if (!bindings) {
            bindings = new Map();
            _webglQueryBindings.set(ctx, bindings);
        }
        return bindings;
    }

    function _webgl2SamplerBindingsFor(ctx) {
        let bindings = _webglSamplerBindings.get(ctx);
        if (!bindings) {
            bindings = new Map();
            _webglSamplerBindings.set(ctx, bindings);
        }
        return bindings;
    }

    function _setWebGL2SamplerParameter(ctx, sampler, pname, param, operation) {
        const state = _requireWebGL2Object('WebGLSampler', _webglSamplerState, sampler, false, operation);
        if (!state || state.context !== ctx || state.deleted) {
            _setWebGLError(ctx, WebGLRenderingContext.INVALID_OPERATION);
            return;
        }
        pname = Number(pname) >>> 0;
        if (!state.parameters.has(pname) || !Number.isFinite(param)) {
            _setWebGLError(ctx, WebGLRenderingContext.INVALID_ENUM);
            return;
        }
        state.parameters.set(pname, param);
    }

    function _compileWebGLShaderSource(source) {
        const text = String(source || '');
        // This is a validation model rather than a renderer/compiler. Cover the
        // structural failures browser code relies on while accepting the broad
        // GLSL ES syntax used by real sites. Pixel rendering remains handled by
        // the existing Canvas/WebGL backend.
        if (!/\bvoid\s+main\s*\(/.test(text)) {
            return { ok: false, log: "ERROR: 0:1: 'main' : function is not defined\n\0" };
        }
        if (/\bthis\b/.test(text)) {
            return { ok: false, log: "ERROR: 0:1: syntax error\n\0" };
        }
        let depth = 0;
        for (const ch of text) {
            if (ch === '{') depth++;
            else if (ch === '}') {
                depth--;
                if (depth < 0) return { ok: false, log: "ERROR: 0:1: syntax error\n\0" };
            }
        }
        if (depth !== 0) return { ok: false, log: "ERROR: 0:1: syntax error\n\0" };
        return { ok: true, log: '' };
    }

    function _webGLDeclaredNames(source, keyword) {
        const names = [];
        const seen = new Set();
        const re = new RegExp(`\\b${keyword}\\s+(?:(?:lowp|mediump|highp)\\s+)?[A-Za-z_]\\w*\\s+([A-Za-z_]\\w*)`, 'g');
        let match;
        while ((match = re.exec(source || ''))) {
            if (!seen.has(match[1])) {
                seen.add(match[1]);
                names.push(match[1]);
            }
        }
        return names;
    }

    // WebGL — routes through Canvas2D backend for real pixel output.
    // Some scripts call readPixels() after clearColor()+clear() and expect real data.
    class WebGLRenderingContext {
        // WebGL constants
        static COLOR_BUFFER_BIT = 0x4000;
        static DEPTH_BUFFER_BIT = 0x0100;
        static STENCIL_BUFFER_BIT = 0x0400;
        static TRIANGLES = 4;
        static TRIANGLE_STRIP = 5;
        static TRIANGLE_FAN = 6;
        static LINES = 1;
        static LINE_STRIP = 3;
        static POINTS = 0;
        static RGBA = 0x1908;
        static UNSIGNED_BYTE = 0x1401;
        static FLOAT = 0x1406;
        static ARRAY_BUFFER = 0x8892;
        static ELEMENT_ARRAY_BUFFER = 0x8893;
        static ARRAY_BUFFER_BINDING = 0x8894;
        static ELEMENT_ARRAY_BUFFER_BINDING = 0x8895;
        static BUFFER_SIZE = 0x8764;
        static BUFFER_USAGE = 0x8765;
        static STREAM_DRAW = 0x88E0;
        static STATIC_DRAW = 0x88E4;
        static DYNAMIC_DRAW = 0x88E8;
        static TEXTURE_2D = 0x0DE1;
        static TEXTURE = 0x1702;
        static TEXTURE_BINDING_2D = 0x8069;
        static TEXTURE_CUBE_MAP = 0x8513;
        static TEXTURE_BINDING_CUBE_MAP = 0x8514;
        static TEXTURE_CUBE_MAP_POSITIVE_X = 0x8515;
        static TEXTURE_CUBE_MAP_NEGATIVE_X = 0x8516;
        static TEXTURE_CUBE_MAP_POSITIVE_Y = 0x8517;
        static TEXTURE_CUBE_MAP_NEGATIVE_Y = 0x8518;
        static TEXTURE_CUBE_MAP_POSITIVE_Z = 0x8519;
        static TEXTURE_CUBE_MAP_NEGATIVE_Z = 0x851A;
        static ACTIVE_TEXTURE = 0x84E0;
        static TEXTURE0 = 0x84C0;
        static TEXTURE1 = 0x84C1;
        static TEXTURE2 = 0x84C2;
        static TEXTURE3 = 0x84C3;
        static TEXTURE4 = 0x84C4;
        static TEXTURE5 = 0x84C5;
        static TEXTURE6 = 0x84C6;
        static TEXTURE7 = 0x84C7;
        static TEXTURE8 = 0x84C8;
        static TEXTURE9 = 0x84C9;
        static TEXTURE10 = 0x84CA;
        static TEXTURE11 = 0x84CB;
        static TEXTURE12 = 0x84CC;
        static TEXTURE13 = 0x84CD;
        static TEXTURE14 = 0x84CE;
        static TEXTURE15 = 0x84CF;
        static TEXTURE16 = 0x84D0;
        static TEXTURE17 = 0x84D1;
        static TEXTURE18 = 0x84D2;
        static TEXTURE19 = 0x84D3;
        static TEXTURE20 = 0x84D4;
        static TEXTURE21 = 0x84D5;
        static TEXTURE22 = 0x84D6;
        static TEXTURE23 = 0x84D7;
        static TEXTURE24 = 0x84D8;
        static TEXTURE25 = 0x84D9;
        static TEXTURE26 = 0x84DA;
        static TEXTURE27 = 0x84DB;
        static TEXTURE28 = 0x84DC;
        static TEXTURE29 = 0x84DD;
        static TEXTURE30 = 0x84DE;
        static TEXTURE31 = 0x84DF;
        static TEXTURE_MAG_FILTER = 0x2800;
        static TEXTURE_MIN_FILTER = 0x2801;
        static TEXTURE_WRAP_S = 0x2802;
        static TEXTURE_WRAP_T = 0x2803;
        static NEAREST = 0x2600;
        static LINEAR = 0x2601;
        static NEAREST_MIPMAP_NEAREST = 0x2700;
        static LINEAR_MIPMAP_NEAREST = 0x2701;
        static NEAREST_MIPMAP_LINEAR = 0x2702;
        static LINEAR_MIPMAP_LINEAR = 0x2703;
        static REPEAT = 0x2901;
        static CLAMP_TO_EDGE = 0x812F;
        static MIRRORED_REPEAT = 0x8370;
        static FRAMEBUFFER = 0x8D40;
        static RENDERBUFFER = 0x8D41;
        static FRAMEBUFFER_BINDING = 0x8CA6;
        static RENDERBUFFER_BINDING = 0x8CA7;
        static FRAMEBUFFER_COMPLETE = 0x8CD5;
        static FRAMEBUFFER_INCOMPLETE_ATTACHMENT = 0x8CD6;
        static FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT = 0x8CD7;
        static FRAMEBUFFER_INCOMPLETE_DIMENSIONS = 0x8CD9;
        static COLOR_ATTACHMENT0 = 0x8CE0;
        static DEPTH_ATTACHMENT = 0x8D00;
        static STENCIL_ATTACHMENT = 0x8D20;
        static DEPTH_STENCIL_ATTACHMENT = 0x821A;
        static FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE = 0x8CD0;
        static FRAMEBUFFER_ATTACHMENT_OBJECT_NAME = 0x8CD1;
        static FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL = 0x8CD2;
        static FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE = 0x8CD3;
        static RENDERBUFFER_WIDTH = 0x8D42;
        static RENDERBUFFER_HEIGHT = 0x8D43;
        static RENDERBUFFER_INTERNAL_FORMAT = 0x8D44;
        static RENDERBUFFER_RED_SIZE = 0x8D50;
        static RENDERBUFFER_GREEN_SIZE = 0x8D51;
        static RENDERBUFFER_BLUE_SIZE = 0x8D52;
        static RENDERBUFFER_ALPHA_SIZE = 0x8D53;
        static RENDERBUFFER_DEPTH_SIZE = 0x8D54;
        static RENDERBUFFER_STENCIL_SIZE = 0x8D55;
        static RGBA4 = 0x8056;
        static RGB5_A1 = 0x8057;
        static RGB565 = 0x8D62;
        static DEPTH_COMPONENT16 = 0x81A5;
        static STENCIL_INDEX8 = 0x8D48;
        static DEPTH_STENCIL = 0x84F9;
        static FRAGMENT_SHADER = 0x8B30;
        static VERTEX_SHADER = 0x8B31;
        static DELETE_STATUS = 0x8B80;
        static COMPILE_STATUS = 0x8B81;
        static LINK_STATUS = 0x8B82;
        static VALIDATE_STATUS = 0x8B83;
        static ATTACHED_SHADERS = 0x8B85;
        static ACTIVE_UNIFORMS = 0x8B86;
        static ACTIVE_ATTRIBUTES = 0x8B89;
        static SHADER_TYPE = 0x8B4F;
        static CURRENT_PROGRAM = 0x8B8D;
        static NO_ERROR = 0;
        static INVALID_ENUM = 0x0500;
        static INVALID_VALUE = 0x0501;
        static INVALID_OPERATION = 0x0502;
        // Parameter pname constants — scripts call e.g. gl.getParameter(gl.MAX_TEXTURE_SIZE).
        static VENDOR = 0x1F00;
        static RENDERER = 0x1F01;
        static VERSION = 0x1F02;
        static SHADING_LANGUAGE_VERSION = 0x8B8C;
        static MAX_TEXTURE_SIZE = 0x0D33;
        static MAX_CUBE_MAP_TEXTURE_SIZE = 0x851C;
        static MAX_RENDERBUFFER_SIZE = 0x84E8;
        static MAX_3D_TEXTURE_SIZE = 0x8073;
        static MAX_VERTEX_ATTRIBS = 0x8869;
        static MAX_VERTEX_UNIFORM_VECTORS = 0x8DFB;
        static MAX_VARYING_VECTORS = 0x8DFD;
        static MAX_FRAGMENT_UNIFORM_VECTORS = 0x8DFC;
        static MAX_TEXTURE_IMAGE_UNITS = 0x8872;
        static MAX_VERTEX_TEXTURE_IMAGE_UNITS = 0x8B4D;
        static MAX_COMBINED_TEXTURE_IMAGE_UNITS = 0x8B4C;
        static ALIASED_POINT_SIZE_RANGE = 0x846D;
        static ALIASED_LINE_WIDTH_RANGE = 0x846E;
        static MAX_VIEWPORT_DIMS = 0x0D3A;
        static DEPTH_BITS = 0x0D56;
        static STENCIL_BITS = 0x0D57;
        static SAMPLE_BUFFERS = 0x80AA;
        static SAMPLES = 0x80A9;
        // Shader-precision-format types
        static LOW_FLOAT = 0x8DF0;
        static MEDIUM_FLOAT = 0x8DF1;
        static HIGH_FLOAT = 0x8DF2;
        static LOW_INT = 0x8DF3;
        static MEDIUM_INT = 0x8DF4;
        static HIGH_INT = 0x8DF5;

        constructor(canvasId, width, height) {
            _configureWebGLContext(this, {
                canvasId,
                width: width === undefined ? 300 : width,
                height: height === undefined ? 150 : height,
            });
        }

        get canvas() { return (_webglState(this) || {}).canvas || null; }
        get drawingBufferWidth() { return _webglDrawingWidth(this); }
        get drawingBufferHeight() { return _webglDrawingHeight(this); }

        // --- Real operations via Canvas2D backend ---
        clearColor(r, g, b, a) {
            const state = _webglState(this);
            if (state) state.clearColor = [Math.round(r*255), Math.round(g*255), Math.round(b*255), a];
        }
        clear(mask) {
            const state = _webglState(this);
            if (mask & 0x4000 && state && state.canvasId !== undefined) { // COLOR_BUFFER_BIT
                const [r, g, b, a] = state.clearColor;
                const width = _webglDrawingWidth(this);
                const height = _webglDrawingHeight(this);
                _debugCanvas({ op: 'webglClear', width, height, rgba: [r, g, b, a] });
                const color = `rgba(${r},${g},${b},${a})`;
                ops.op_canvas_set_fill_style(state.canvasId, color);
                ops.op_canvas_fill_rect(state.canvasId, 0, 0, width, height);
            }
        }
        readPixels(x, y, w, h, format, type, pixels) {
            const state = _webglState(this);
            if (!state || state.canvasId === undefined || !pixels) return;
            // Canvas2D stores pixels top-down, WebGL is bottom-up — flip Y
            const flippedY = _webglDrawingHeight(this) - y - h;
            const data = ops.op_canvas_get_image_data(state.canvasId, x, Math.max(0, flippedY), w, h);
            for (let i = 0; i < data.length && i < pixels.length; i++) {
                pixels[i] = data[i];
            }
        }
        viewport(x, y, w, h) {
            const state = _webglState(this);
            if (state) state.viewport = [Number(x), Number(y), Number(w), Number(h)];
        }

        // --- Parameter queries (fingerprint-relevant values) ---
        //
        // All values come from the active StealthProfile's gpu_profile entry.
        // Loaded lazily the first time getParameter is called and cached on
        // the WebGLRenderingContext constructor itself (shared across
        // instances). Implementation note: this is now a STATIC accessor
        // wrapper around a closure-scoped cache loader so the methods
        // below don't reference `this._g` — that meant
        // `getParameter.call(somethingElse)` threw
        // `TypeError: this._g is not a function`, which some scripts
        // detect. Real Chrome's native methods don't have that dependency.
        static _g() {
            if (WebGLRenderingContext._gpuCache) return WebGLRenderingContext._gpuCache;
            // Defaults — used when no stealth profile is active. Must match
            // stealth::gpu::common_params_desktop() so probes that check for
            // non-zero MAX_TEXTURE_SIZE etc. don't see `null` in headless mode.
            // Defaults match captured Chrome 147 on macOS arm64
            // (tests/fixtures/chrome147/captured_macos_arm64.json).
            let vendor = "WebKit";
            let renderer = "WebKit WebGL";
            let version = "WebGL 2.0 (OpenGL ES 3.0 Chromium)";
            let shadingLang = "WebGL GLSL ES 3.00 (OpenGL ES GLSL ES 3.0 Chromium)";
            let unmaskedVendor = "Google Inc. (Apple)";
            let unmaskedRenderer = "ANGLE (Apple, ANGLE Metal Renderer: Apple M3, Unspecified Version)";
            let extensions = [];
            let params = {
                0x0D33: 16384,         // MAX_TEXTURE_SIZE
                0x851C: 16384,         // MAX_CUBE_MAP_TEXTURE_SIZE
                0x84E8: 16384,         // MAX_RENDERBUFFER_SIZE
                0x8073: 2048,          // MAX_3D_TEXTURE_SIZE
                0x8869: 16,            // MAX_VERTEX_ATTRIBS
                0x8DFB: 1024,          // MAX_VERTEX_UNIFORM_VECTORS
                0x8DFD: 15,            // MAX_VARYING_VECTORS
                0x8DFC: 1024,          // MAX_FRAGMENT_UNIFORM_VECTORS
                0x8872: 16,            // MAX_TEXTURE_IMAGE_UNITS
                0x8B4D: 16,            // MAX_VERTEX_TEXTURE_IMAGE_UNITS
                0x8B4C: 32,            // MAX_COMBINED_TEXTURE_IMAGE_UNITS
                // ALIASED_POINT_SIZE_RANGE — captured Chrome 147 macOS: [1, 511] typical
                0x846D: [1.0, 511.0],
                0x846E: [1.0, 1.0],    // ALIASED_LINE_WIDTH_RANGE — Chrome ANGLE on every OS = [1,1]
                0x0D3A: [16384, 16384],// MAX_VIEWPORT_DIMS — captured Chrome 147 macOS
                0x0D56: 8,             // DEPTH_BITS
                0x0D57: 8,             // STENCIL_BITS
                0x80AA: 2,             // SAMPLE_BUFFERS
                0x80A9: 4,             // SAMPLES
            };
            let shaderPrec = {};
            try {
                if (ops.op_has_stealth_profile()) {
                    const s = (k) => ops.op_get_profile_value(k);
                    unmaskedVendor = s("webgl_unmasked_vendor") || unmaskedVendor;
                    unmaskedRenderer = s("webgl_unmasked_renderer") || unmaskedRenderer;
                    version = s("webgl_version") || version;
                    shadingLang = s("webgl_shading_language_version") || shadingLang;
                    const extsJson = s("webgl_extensions");
                    if (extsJson) {
                        try { extensions = JSON.parse(extsJson); } catch {}
                    }
                    const paramsJson = s("webgl_params");
                    if (paramsJson) {
                        try {
                            const arr = JSON.parse(paramsJson);
                            // Array of [glenum, value] pairs → keyed object
                            for (const [k, v] of arr) params[k] = v;
                        } catch {}
                    }
                    const spJson = s("webgl_shader_precision");
                    if (spJson) {
                        try {
                            // Array of [shader_type, precision_type, [min, max, precision]]
                            const arr = JSON.parse(spJson);
                            for (const [st, pt, v] of arr) {
                                shaderPrec[`${st}:${pt}`] = { rangeMin: v[0], rangeMax: v[1], precision: v[2] };
                            }
                        } catch {}
                    }
                }
            } catch {}
            // Firefox WebGL coherence: Gecko reports "Mozilla" for VENDOR /
            // RENDERER and the UNMASKED_*_WEBGL strings, and a VERSION without
            // the "(OpenGL ES … Chromium)" suffix. Chrome's GL identity
            // ("WebKit" / "Google Inc." / "ANGLE (…)") under a Firefox UA is a
            // 100% tell, so override the whole GL identity for the FF profile.
            try {
                if (ops.op_has_stealth_profile() &&
                    /Firefox\//.test(ops.op_get_profile_value("user_agent") || "")) {
                    vendor = "Mozilla";
                    renderer = "Mozilla";
                    unmaskedVendor = "Mozilla";
                    unmaskedRenderer = "Mozilla";
                    version = "WebGL 2.0";
                    shadingLang = "WebGL GLSL ES 3.00";
                }
            } catch {}
            WebGLRenderingContext._gpuCache = {
                vendor, renderer, version, shadingLang,
                unmaskedVendor, unmaskedRenderer,
                extensions, params, shaderPrec,
            };
            return WebGLRenderingContext._gpuCache;
        }
        // FIX-D2: the WebGL **1.0** surface. `_g()` above is the WebGL **2.0**
        // surface; a `getContext("webgl")` context must NOT report the WebGL 2
        // version string or expose WebGL-2-only extensions (e.g.
        // `EXT_color_buffer_float`) — that cross-API mismatch differs from
        // real Chrome. Derived from the active
        // profile's `webgl1_*` values; falls back to `_g()` when the profile has
        // no distinct WebGL 1 surface (legacy profiles) → no behaviour change.
        static _g1() {
            if (WebGLRenderingContext._gpuCache1) return WebGLRenderingContext._gpuCache1;
            const base = WebGLRenderingContext._g();
            // Start from the base surface, then downgrade the version strings to
            // WebGL 1 whenever base describes a WebGL 2 surface (the apple_m3
            // default + the no-profile fallback). Legacy profiles whose shared
            // field already holds WebGL 1 data (e.g. nvidia) or masked Firefox
            // keep their base strings. Extensions: an empty list defers to
            // getSupportedExtensions()'s own WebGL-1 fallback.
            let version = base.version;
            let shadingLang = base.shadingLang;
            let extensions = base.extensions;
            const _ffWebGL = (function () {
                try { return ops.op_has_stealth_profile() && /Firefox\//.test(ops.op_get_profile_value("user_agent") || ""); }
                catch { return false; }
            })();
            if (/^WebGL 2/.test(version)) {
                // Firefox WebGL 1 reports "WebGL 1.0" with no "(OpenGL ES … Chromium)" suffix.
                version = _ffWebGL ? "WebGL 1.0" : "WebGL 1.0 (OpenGL ES 2.0 Chromium)";
                shadingLang = _ffWebGL ? "WebGL GLSL ES 1.0" : "WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)";
            }
            try {
                if (ops.op_has_stealth_profile()) {
                    const v = ops.op_get_profile_value("webgl1_version");
                    const sl = ops.op_get_profile_value("webgl1_shading_language_version");
                    const extJson = ops.op_get_profile_value("webgl1_extensions");
                    if (v) version = v;
                    if (sl) shadingLang = sl;
                    if (extJson) {
                        try { const e = JSON.parse(extJson); if (e && e.length) extensions = e; } catch {}
                    }
                }
            } catch {}
            WebGLRenderingContext._gpuCache1 = { ...base, version, shadingLang, extensions };
            return WebGLRenderingContext._gpuCache1;
        }
        // Per-instance surface selector. `_isWebGL2 === false` only for a
        // context handed back by `getContext("webgl"/"experimental-webgl")`.
        // Anything else (incl. `getParameter.call(notACtx)`) → WebGL 2 surface,
        // preserving the pre-FIX-D2 default.
        static _surfaceFor(ctx) {
            const state = ctx && _webglState(ctx);
            return (state && state.isWebGL2 === false)
                ? WebGLRenderingContext._g1()
                : WebGLRenderingContext._g();
        }
        getParameter(pname) {
            const gpu = WebGLRenderingContext._surfaceFor(this);
            // String-valued parameters
            if (pname === 0x1F00) return gpu.vendor;                // VENDOR
            if (pname === 0x1F01) return gpu.renderer;              // RENDERER
            if (pname === 0x1F02) return gpu.version;               // VERSION
            if (pname === 0x8B8C) return gpu.shadingLang;           // SHADING_LANGUAGE_VERSION
            if (pname === 0x9245) return gpu.unmaskedVendor;        // UNMASKED_VENDOR_WEBGL
            if (pname === 0x9246) return gpu.unmaskedRenderer;      // UNMASKED_RENDERER_WEBGL
            if (pname === WebGLRenderingContext.CURRENT_PROGRAM) {
                return _webglCurrentProgram.get(this) || null;
            }
            if (pname === WebGLRenderingContext.ARRAY_BUFFER_BINDING) {
                return _webglBindings(this).array;
            }
            if (pname === WebGLRenderingContext.ELEMENT_ARRAY_BUFFER_BINDING) {
                return _webglBindings(this).element;
            }
            if (pname === WebGLRenderingContext.ACTIVE_TEXTURE) {
                return WebGLRenderingContext.TEXTURE0 + _webglTextureBindingState(this).activeUnit;
            }
            if (pname === WebGLRenderingContext.TEXTURE_BINDING_2D) {
                return _webglBoundTexture(this, WebGLRenderingContext.TEXTURE_2D) || null;
            }
            if (pname === WebGLRenderingContext.TEXTURE_BINDING_CUBE_MAP) {
                return _webglBoundTexture(this, WebGLRenderingContext.TEXTURE_CUBE_MAP) || null;
            }
            if (pname === WebGLRenderingContext.FRAMEBUFFER_BINDING) {
                return _webglFramebufferBinding.get(this) || null;
            }
            if (pname === WebGLRenderingContext.RENDERBUFFER_BINDING) {
                return _webglRenderbufferBinding.get(this) || null;
            }
            // Runtime-dependent values (not from the catalog)
            if (pname === 0x0BA2) {
                const state = _webglState(this);
                return state ? state.viewport.slice() : [0, 0, 0, 0]; // VIEWPORT
            }
            // Catalog-sourced numeric/array parameters
            if (gpu.params[pname] !== undefined) return gpu.params[pname];
            return null;
        }
        getSupportedExtensions() {
            const gpu = WebGLRenderingContext._surfaceFor(this);
            // Fallback if the catalog is empty (no profile active).
            // Captured from real Chrome 147 on macOS arm64. WebGL 1 contexts get
            // the WebGL-1 list (extensions promoted to core in WebGL 2 reappear;
            // WebGL-2-only ones absent); WebGL 2 contexts get the 36-ext list.
            if (!gpu.extensions.length) {
                const state = this && _webglState(this);
                if (state && state.isWebGL2 === false) {
                    return [
                        "ANGLE_instanced_arrays","EXT_blend_minmax","EXT_clip_control",
                        "EXT_color_buffer_half_float","EXT_depth_clamp","EXT_disjoint_timer_query",
                        "EXT_float_blend","EXT_frag_depth","EXT_polygon_offset_clamp","EXT_sRGB",
                        "EXT_shader_texture_lod","EXT_texture_compression_bptc",
                        "EXT_texture_compression_rgtc","EXT_texture_filter_anisotropic",
                        "EXT_texture_mirror_clamp_to_edge","KHR_parallel_shader_compile",
                        "OES_element_index_uint","OES_fbo_render_mipmap","OES_standard_derivatives",
                        "OES_texture_float","OES_texture_float_linear","OES_texture_half_float",
                        "OES_texture_half_float_linear","OES_vertex_array_object",
                        "WEBGL_blend_func_extended","WEBGL_color_buffer_float",
                        "WEBGL_compressed_texture_astc","WEBGL_compressed_texture_etc",
                        "WEBGL_compressed_texture_etc1","WEBGL_compressed_texture_pvrtc",
                        "WEBGL_compressed_texture_s3tc","WEBGL_compressed_texture_s3tc_srgb",
                        "WEBGL_debug_renderer_info","WEBGL_debug_shaders","WEBGL_depth_texture",
                        "WEBGL_draw_buffers","WEBGL_lose_context","WEBGL_multi_draw",
                        "WEBGL_polygon_mode",
                    ];
                }
                return [
                    "EXT_clip_control","EXT_color_buffer_float","EXT_color_buffer_half_float",
                    "EXT_conservative_depth","EXT_depth_clamp","EXT_disjoint_timer_query_webgl2",
                    "EXT_float_blend","EXT_polygon_offset_clamp","EXT_render_snorm",
                    "EXT_texture_compression_bptc","EXT_texture_compression_rgtc",
                    "EXT_texture_filter_anisotropic","EXT_texture_mirror_clamp_to_edge",
                    "EXT_texture_norm16","KHR_parallel_shader_compile",
                    "NV_shader_noperspective_interpolation","OES_draw_buffers_indexed",
                    "OES_sample_variables","OES_shader_multisample_interpolation",
                    "OES_texture_float_linear","WEBGL_blend_func_extended",
                    "WEBGL_clip_cull_distance","WEBGL_compressed_texture_astc",
                    "WEBGL_compressed_texture_etc","WEBGL_compressed_texture_etc1",
                    "WEBGL_compressed_texture_pvrtc","WEBGL_compressed_texture_s3tc",
                    "WEBGL_compressed_texture_s3tc_srgb","WEBGL_debug_renderer_info",
                    "WEBGL_debug_shaders","WEBGL_lose_context","WEBGL_multi_draw",
                    "WEBGL_polygon_mode","WEBGL_provoking_vertex",
                    "WEBGL_render_shared_exponent","WEBGL_stencil_texturing",
                ];
            }
            return gpu.extensions.slice();
        }
        getExtension(name) {
            return _getWebGLExtensionObject(this, name);
        }
        // getContextAttributes — returns the WebGLContextAttributes used at
        // creation. Real Chrome returns these specific defaults.
        getContextAttributes() {
            return {
                alpha: true,
                antialias: true,
                depth: true,
                failIfMajorPerformanceCaveat: false,
                powerPreference: "default",
                premultipliedAlpha: true,
                preserveDrawingBuffer: false,
                stencil: false,
                desynchronized: false,
                xrCompatible: false,
            };
        }
        isContextLost() { return false; }
        getShaderPrecisionFormat(shaderType, precisionType) {
            const gpu = WebGLRenderingContext._g();
            const key = `${shaderType}:${precisionType}`;
            if (gpu.shaderPrec[key]) return gpu.shaderPrec[key];
            // Fallback for unknown combinations — float-style values (our old behavior)
            return { rangeMin: 127, rangeMax: 127, precision: 23 };
        }

        // --- Shader/program lifecycle ---
        createShader(type) {
            type = Number(type) >>> 0;
            if (type !== WebGLRenderingContext.VERTEX_SHADER && type !== WebGLRenderingContext.FRAGMENT_SHADER) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            return _newWebGLObject('WebGLShader', _webglShaderState, {
                context: this,
                type,
                source: '',
                compiled: false,
                deleted: false,
                attachments: new Set(),
                infoLog: '',
            });
        }
        shaderSource(shader, source) {
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!state || state.context !== this || (state.deleted && state.attachments.size === 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            state.source = String(source);
            state.compiled = false;
            state.infoLog = '';
        }
        getShaderSource(shader) {
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!state || state.context !== this || (state.deleted && state.attachments.size === 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            return state.source;
        }
        compileShader(shader) {
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!state || state.context !== this || (state.deleted && state.attachments.size === 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            const result = _compileWebGLShaderSource(state.source);
            state.compiled = result.ok;
            state.infoLog = result.log;
        }
        getShaderInfoLog(shader) {
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!state || state.context !== this || (state.deleted && state.attachments.size === 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            return state.infoLog;
        }
        getShaderParameter(shader, pname) {
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!state || state.context !== this || (state.deleted && state.attachments.size === 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            switch (Number(pname) >>> 0) {
                case WebGLRenderingContext.DELETE_STATUS: return state.deleted;
                case WebGLRenderingContext.COMPILE_STATUS: return state.compiled;
                case WebGLRenderingContext.SHADER_TYPE: return state.type;
                default:
                    _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                    return null;
            }
        }
        isShader(shader) {
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader, false, 'isShader');
            return !!state && state.context === this && (!state.deleted || state.attachments.size > 0);
        }
        createProgram() {
            return _newWebGLObject('WebGLProgram', _webglProgramState, {
                context: this,
                attached: new Set(),
                linked: false,
                validated: false,
                deleted: false,
                infoLog: '',
                attribBindings: new Map(),
                attribLocations: new Map(),
                uniforms: new Set(),
            });
        }
        attachShader(program, shader) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            const s = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!p || !s || p.context !== this || s.context !== this || p.deleted || (s.deleted && s.attachments.size === 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            if (p.attached.has(shader)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            p.attached.add(shader);
            s.attachments.add(program);
            p.linked = false;
            p.validated = false;
        }
        detachShader(program, shader) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            const s = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!p || !s || p.context !== this || s.context !== this || p.deleted || !p.attached.has(shader)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            p.attached.delete(shader);
            s.attachments.delete(program);
            p.linked = false;
            p.validated = false;
        }
        getAttachedShaders(program) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            return Array.from(p.attached);
        }
        bindAttribLocation(program, index, name) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            index = Number(index) >>> 0;
            name = String(name);
            if (index >= Number(this.getParameter(WebGLRenderingContext.MAX_VERTEX_ATTRIBS) || 0)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            p.attribBindings.set(name, index);
        }
        linkProgram(program) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            const shaders = Array.from(p.attached).map((shader) => [shader, _webglShaderState.get(shader)]);
            const vertex = shaders.find(([, s]) => s && s.type === WebGLRenderingContext.VERTEX_SHADER);
            const fragment = shaders.find(([, s]) => s && s.type === WebGLRenderingContext.FRAGMENT_SHADER);
            if (!vertex && !fragment) {
                p.linked = false;
                p.infoLog = 'No compiled shaders.\n\0';
                return;
            }
            if (!vertex || !fragment) {
                p.linked = false;
                p.infoLog = 'The program must contain objects to form both a vertex and fragment shader.\n\0';
                return;
            }
            if (!vertex[1].compiled || !fragment[1].compiled) {
                p.linked = false;
                p.infoLog = 'Attached shaders must be successfully compiled.\n\0';
                return;
            }
            p.linked = true;
            p.validated = false;
            p.infoLog = '';
            p.attribLocations.clear();
            const attributes = _webGLDeclaredNames(vertex[1].source, '(?:attribute|in)');
            const used = new Set();
            for (const name of attributes) {
                if (p.attribBindings.has(name)) {
                    const index = p.attribBindings.get(name);
                    p.attribLocations.set(name, index);
                    used.add(index);
                }
            }
            let next = 0;
            for (const name of attributes) {
                if (p.attribLocations.has(name)) continue;
                while (used.has(next)) next++;
                p.attribLocations.set(name, next);
                used.add(next++);
            }
            p.uniforms = new Set([
                ..._webGLDeclaredNames(vertex[1].source, 'uniform'),
                ..._webGLDeclaredNames(fragment[1].source, 'uniform'),
            ]);
        }
        validateProgram(program) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            p.validated = p.linked;
        }
        getProgramInfoLog(program) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            const retained = p && p.deleted && _webglCurrentProgram.get(this) === program;
            if (!p || p.context !== this || (p.deleted && !retained)) return null;
            return p.infoLog;
        }
        getProgramParameter(program, pname) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            const retained = p && p.deleted && _webglCurrentProgram.get(this) === program;
            if (!p || p.context !== this || (p.deleted && !retained)) return null;
            switch (Number(pname) >>> 0) {
                case WebGLRenderingContext.DELETE_STATUS: return p.deleted;
                case WebGLRenderingContext.LINK_STATUS: return p.linked;
                case WebGLRenderingContext.VALIDATE_STATUS: return p.validated;
                case WebGLRenderingContext.ATTACHED_SHADERS: return p.attached.size;
                case WebGLRenderingContext.ACTIVE_ATTRIBUTES: return p.linked ? p.attribLocations.size : 0;
                case WebGLRenderingContext.ACTIVE_UNIFORMS: return p.linked ? p.uniforms.size : 0;
                default:
                    _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                    return null;
            }
        }
        isProgram(program) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program, false, 'isProgram');
            return !!p && p.context === this && (!p.deleted || _webglCurrentProgram.get(this) === program);
        }
        useProgram(program) {
            if (program == null) {
                _webglCurrentProgram.set(this, null);
                return;
            }
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted || !p.linked) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            _webglCurrentProgram.set(this, program);
        }
        getUniformLocation(program, name) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            if (!p.linked) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return null;
            }
            name = String(name);
            if (!p.uniforms.has(name)) return null;
            return _newWebGLObject('WebGLUniformLocation', _webglUniformLocationState, {
                context: this,
                program,
                name,
            });
        }
        getAttribLocation(program, name) {
            const p = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!p || p.context !== this || p.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return -1;
            }
            if (!p.linked) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return -1;
            }
            name = String(name);
            return p.attribLocations.has(name) ? p.attribLocations.get(name) : -1;
        }
        uniform1f() {}
        uniform1i() {}
        uniform2f() {}
        uniform3f() {}
        uniform4f() {}
        uniformMatrix4fv() {}
        createBuffer() {
            return _newWebGLObject('WebGLBuffer', _webglBufferState, {
                context: this,
                deleted: false,
                everBound: false,
                size: 0,
                usage: 0,
            });
        }
        bindBuffer(target, buffer) {
            target = Number(target) >>> 0;
            let slot;
            if (target === WebGLRenderingContext.ARRAY_BUFFER) slot = 'array';
            else if (target === WebGLRenderingContext.ELEMENT_ARRAY_BUFFER) slot = 'element';
            else {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            if (buffer == null) {
                _webglBindings(this)[slot] = null;
                return;
            }
            const state = _requireWebGLObject('WebGLBuffer', _webglBufferState, buffer, false, 'bindBuffer');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.everBound = true;
            _webglBindings(this)[slot] = buffer;
        }
        bufferData(target, dataOrSize, usage) {
            target = Number(target) >>> 0;
            let slot;
            if (target === WebGLRenderingContext.ARRAY_BUFFER) slot = 'array';
            else if (target === WebGLRenderingContext.ELEMENT_ARRAY_BUFFER) slot = 'element';
            else {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            usage = Number(usage) >>> 0;
            if (![WebGLRenderingContext.STREAM_DRAW, WebGLRenderingContext.STATIC_DRAW, WebGLRenderingContext.DYNAMIC_DRAW].includes(usage)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const buffer = _webglBindings(this)[slot];
            const state = buffer && _webglBufferState.get(buffer);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            let size;
            if (typeof dataOrSize === 'number') {
                size = Number(dataOrSize);
                if (!Number.isFinite(size) || size < 0) {
                    _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                    return;
                }
                size = Math.trunc(size);
            } else if (dataOrSize && typeof dataOrSize.byteLength === 'number') {
                size = Number(dataOrSize.byteLength);
            } else {
                throw new TypeError("Failed to execute 'bufferData' on 'WebGLRenderingContext': parameter 2 is not of type 'ArrayBufferView'.");
            }
            state.size = size;
            state.usage = usage;
        }
        getBufferParameter(target, pname) {
            target = Number(target) >>> 0;
            let slot;
            if (target === WebGLRenderingContext.ARRAY_BUFFER) slot = 'array';
            else if (target === WebGLRenderingContext.ELEMENT_ARRAY_BUFFER) slot = 'element';
            else {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            const buffer = _webglBindings(this)[slot];
            const state = buffer && _webglBufferState.get(buffer);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return null;
            }
            pname = Number(pname) >>> 0;
            if (pname === WebGLRenderingContext.BUFFER_SIZE) return state.size;
            if (pname === WebGLRenderingContext.BUFFER_USAGE) return state.usage;
            _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
            return null;
        }
        isBuffer(buffer) {
            const state = _requireWebGLObject('WebGLBuffer', _webglBufferState, buffer, false, 'isBuffer');
            return !!state && state.context === this && !state.deleted && state.everBound;
        }
        enableVertexAttribArray() {}
        disableVertexAttribArray() {}
        vertexAttribPointer() {}
        drawArrays() {}
        drawElements() {}
        createTexture() {
            return _newWebGLObject('WebGLTexture', _webglTextureState, {
                context: this,
                deleted: false,
                everBound: false,
                target: 0,
                parameters: new Map([
                    [WebGLRenderingContext.TEXTURE_MAG_FILTER, WebGLRenderingContext.LINEAR],
                    [WebGLRenderingContext.TEXTURE_MIN_FILTER, WebGLRenderingContext.NEAREST_MIPMAP_LINEAR],
                    [WebGLRenderingContext.TEXTURE_WRAP_S, WebGLRenderingContext.REPEAT],
                    [WebGLRenderingContext.TEXTURE_WRAP_T, WebGLRenderingContext.REPEAT],
                ]),
                images: new Map(),
            });
        }
        bindTexture(target, texture) {
            target = Number(target) >>> 0;
            const slot = _webglTextureSlot(target);
            if (!slot) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const bindings = _webglTextureBindingState(this);
            if (texture == null) {
                bindings.units[bindings.activeUnit][slot] = null;
                return;
            }
            const state = _requireWebGLObject('WebGLTexture', _webglTextureState, texture, false, 'bindTexture');
            if (!state || state.context !== this || state.deleted || (state.target && state.target !== target)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.target = target;
            state.everBound = true;
            bindings.units[bindings.activeUnit][slot] = texture;
        }
        texImage2D(target, level, internalformat, widthOrFormat, heightOrType, borderOrSource, ...rest) {
            target = Number(target) >>> 0;
            level = Number(level) | 0;
            const slot = _webglTextureImageBindingSlot(target);
            if (!slot) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const texture = _webglBoundTexture(this, target, true);
            const state = texture && _webglTextureState.get(texture);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            if (level < 0) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            let width = 0;
            let height = 0;
            let format;
            let type;
            if (rest.length >= 3) {
                width = Number(widthOrFormat);
                height = Number(heightOrType);
                const border = Number(borderOrSource);
                format = rest[0];
                type = rest[1];
                if (!Number.isFinite(width) || !Number.isFinite(height) || width < 0 || height < 0 || border !== 0) {
                    _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                    return;
                }
            } else {
                format = widthOrFormat;
                type = heightOrType;
                const source = borderOrSource;
                width = Number(source && (source.videoWidth ?? source.naturalWidth ?? source.width)) || 0;
                height = Number(source && (source.videoHeight ?? source.naturalHeight ?? source.height)) || 0;
            }
            state.images.set(`${target}:${level}`, {
                width: Math.trunc(width),
                height: Math.trunc(height),
                internalformat: Number(internalformat) >>> 0,
                format: Number(format) >>> 0,
                type: Number(type) >>> 0,
            });
        }
        texParameteri(target, pname, param) {
            target = Number(target) >>> 0;
            const texture = _webglBoundTexture(this, target);
            if (texture === undefined) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const state = texture && _webglTextureState.get(texture);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            pname = Number(pname) >>> 0;
            param = Number(param) >>> 0;
            let allowed;
            if (pname === WebGLRenderingContext.TEXTURE_MAG_FILTER) {
                allowed = [WebGLRenderingContext.NEAREST, WebGLRenderingContext.LINEAR];
            } else if (pname === WebGLRenderingContext.TEXTURE_MIN_FILTER) {
                allowed = [
                    WebGLRenderingContext.NEAREST, WebGLRenderingContext.LINEAR,
                    WebGLRenderingContext.NEAREST_MIPMAP_NEAREST, WebGLRenderingContext.LINEAR_MIPMAP_NEAREST,
                    WebGLRenderingContext.NEAREST_MIPMAP_LINEAR, WebGLRenderingContext.LINEAR_MIPMAP_LINEAR,
                ];
            } else if (pname === WebGLRenderingContext.TEXTURE_WRAP_S || pname === WebGLRenderingContext.TEXTURE_WRAP_T) {
                allowed = [WebGLRenderingContext.CLAMP_TO_EDGE, WebGLRenderingContext.MIRRORED_REPEAT, WebGLRenderingContext.REPEAT];
            } else {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            if (!allowed.includes(param)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            state.parameters.set(pname, param);
        }
        getTexParameter(target, pname) {
            target = Number(target) >>> 0;
            const texture = _webglBoundTexture(this, target);
            if (texture === undefined) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            const state = texture && _webglTextureState.get(texture);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return null;
            }
            pname = Number(pname) >>> 0;
            if (!state.parameters.has(pname)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            return state.parameters.get(pname);
        }
        activeTexture(texture) {
            const unit = (Number(texture) >>> 0) - WebGLRenderingContext.TEXTURE0;
            if (unit < 0 || unit >= 32) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            _webglTextureBindingState(this).activeUnit = unit;
        }
        generateMipmap(target) {
            target = Number(target) >>> 0;
            const texture = _webglBoundTexture(this, target);
            if (texture === undefined) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const state = texture && _webglTextureState.get(texture);
            if (!state || state.deleted) _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
        }
        isTexture(texture) {
            const state = _requireWebGLObject('WebGLTexture', _webglTextureState, texture, false, 'isTexture');
            return !!state && state.context === this && !state.deleted && state.everBound;
        }
        createFramebuffer() {
            const framebuffer = _newWebGLObject('WebGLFramebuffer', _webglFramebufferState, {
                context: this,
                deleted: false,
                everBound: false,
                attachments: new Map(),
            });
            _webglFramebuffers(this).add(framebuffer);
            return framebuffer;
        }
        bindFramebuffer(target, framebuffer) {
            target = Number(target) >>> 0;
            if (target !== WebGLRenderingContext.FRAMEBUFFER) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            if (framebuffer == null) {
                _webglFramebufferBinding.set(this, null);
                return;
            }
            const state = _requireWebGLObject('WebGLFramebuffer', _webglFramebufferState, framebuffer, false, 'bindFramebuffer');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.everBound = true;
            _webglFramebufferBinding.set(this, framebuffer);
        }
        framebufferTexture2D(target, attachment, textarget, texture, level) {
            target = Number(target) >>> 0;
            attachment = Number(attachment) >>> 0;
            textarget = Number(textarget) >>> 0;
            level = Number(level) | 0;
            if (target !== WebGLRenderingContext.FRAMEBUFFER || !_webglAttachmentAllowed(attachment) ||
                !_webglTextureImageBindingSlot(textarget)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            if (level !== 0) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            const framebuffer = _webglFramebufferBinding.get(this);
            const framebufferState = framebuffer && _webglFramebufferState.get(framebuffer);
            if (!framebufferState || framebufferState.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            if (texture == null) {
                framebufferState.attachments.delete(attachment);
                return;
            }
            const textureState = _requireWebGLObject('WebGLTexture', _webglTextureState, texture, false, 'framebufferTexture2D');
            if (!textureState || textureState.context !== this || textureState.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            framebufferState.attachments.set(attachment, { kind: 'texture', object: texture, target: textarget, level });
        }
        createRenderbuffer() {
            return _newWebGLObject('WebGLRenderbuffer', _webglRenderbufferState, {
                context: this,
                deleted: false,
                everBound: false,
                width: 0,
                height: 0,
                internalFormat: 0,
            });
        }
        bindRenderbuffer(target, renderbuffer) {
            target = Number(target) >>> 0;
            if (target !== WebGLRenderingContext.RENDERBUFFER) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            if (renderbuffer == null) {
                _webglRenderbufferBinding.set(this, null);
                return;
            }
            const state = _requireWebGLObject('WebGLRenderbuffer', _webglRenderbufferState, renderbuffer, false, 'bindRenderbuffer');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.everBound = true;
            _webglRenderbufferBinding.set(this, renderbuffer);
        }
        renderbufferStorage(target, internalformat, width, height) {
            target = Number(target) >>> 0;
            internalformat = Number(internalformat) >>> 0;
            width = Number(width);
            height = Number(height);
            if (target !== WebGLRenderingContext.RENDERBUFFER) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const validFormats = [
                WebGLRenderingContext.RGBA4, WebGLRenderingContext.RGB5_A1,
                WebGLRenderingContext.RGB565, WebGLRenderingContext.DEPTH_COMPONENT16,
                WebGLRenderingContext.STENCIL_INDEX8, WebGLRenderingContext.DEPTH_STENCIL,
            ];
            if (!validFormats.includes(internalformat)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const renderbuffer = _webglRenderbufferBinding.get(this);
            const state = renderbuffer && _webglRenderbufferState.get(renderbuffer);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            const maxSize = Number(WebGLRenderingContext._surfaceFor(this).params[WebGLRenderingContext.MAX_RENDERBUFFER_SIZE] || 16384);
            if (!Number.isFinite(width) || !Number.isFinite(height) || width < 0 || height < 0 || width > maxSize || height > maxSize) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            state.width = Math.trunc(width);
            state.height = Math.trunc(height);
            state.internalFormat = internalformat;
        }
        getRenderbufferParameter(target, pname) {
            target = Number(target) >>> 0;
            pname = Number(pname) >>> 0;
            if (target !== WebGLRenderingContext.RENDERBUFFER) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            const renderbuffer = _webglRenderbufferBinding.get(this);
            const state = renderbuffer && _webglRenderbufferState.get(renderbuffer);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return null;
            }
            if (pname === WebGLRenderingContext.RENDERBUFFER_WIDTH) return state.width;
            if (pname === WebGLRenderingContext.RENDERBUFFER_HEIGHT) return state.height;
            if (pname === WebGLRenderingContext.RENDERBUFFER_INTERNAL_FORMAT) return state.internalFormat;
            const components = {
                [WebGLRenderingContext.RGBA4]: [4, 4, 4, 4, 0, 0],
                [WebGLRenderingContext.RGB5_A1]: [5, 5, 5, 1, 0, 0],
                [WebGLRenderingContext.RGB565]: [5, 6, 5, 0, 0, 0],
                [WebGLRenderingContext.DEPTH_COMPONENT16]: [0, 0, 0, 0, 16, 0],
                [WebGLRenderingContext.STENCIL_INDEX8]: [0, 0, 0, 0, 0, 8],
                [WebGLRenderingContext.DEPTH_STENCIL]: [0, 0, 0, 0, 24, 8],
            }[state.internalFormat] || [0, 0, 0, 0, 0, 0];
            const componentIndex = new Map([
                [WebGLRenderingContext.RENDERBUFFER_RED_SIZE, 0],
                [WebGLRenderingContext.RENDERBUFFER_GREEN_SIZE, 1],
                [WebGLRenderingContext.RENDERBUFFER_BLUE_SIZE, 2],
                [WebGLRenderingContext.RENDERBUFFER_ALPHA_SIZE, 3],
                [WebGLRenderingContext.RENDERBUFFER_DEPTH_SIZE, 4],
                [WebGLRenderingContext.RENDERBUFFER_STENCIL_SIZE, 5],
            ]).get(pname);
            if (componentIndex !== undefined) return components[componentIndex];
            _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
            return null;
        }
        framebufferRenderbuffer(target, attachment, renderbuffertarget, renderbuffer) {
            target = Number(target) >>> 0;
            attachment = Number(attachment) >>> 0;
            renderbuffertarget = Number(renderbuffertarget) >>> 0;
            if (target !== WebGLRenderingContext.FRAMEBUFFER ||
                renderbuffertarget !== WebGLRenderingContext.RENDERBUFFER ||
                !_webglAttachmentAllowed(attachment)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const framebuffer = _webglFramebufferBinding.get(this);
            const framebufferState = framebuffer && _webglFramebufferState.get(framebuffer);
            if (!framebufferState || framebufferState.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            if (renderbuffer == null) {
                framebufferState.attachments.delete(attachment);
                return;
            }
            const renderbufferState = _requireWebGLObject('WebGLRenderbuffer', _webglRenderbufferState, renderbuffer, false, 'framebufferRenderbuffer');
            if (!renderbufferState || renderbufferState.context !== this || renderbufferState.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            framebufferState.attachments.set(attachment, { kind: 'renderbuffer', object: renderbuffer });
        }
        checkFramebufferStatus(target) {
            target = Number(target) >>> 0;
            if (target !== WebGLRenderingContext.FRAMEBUFFER) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return 0;
            }
            const framebuffer = _webglFramebufferBinding.get(this);
            if (!framebuffer) return WebGLRenderingContext.FRAMEBUFFER_COMPLETE;
            const state = _webglFramebufferState.get(framebuffer);
            if (!state || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return 0;
            }
            if (state.attachments.size === 0) return WebGLRenderingContext.FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT;
            let dimensions = null;
            for (const entry of state.attachments.values()) {
                let width = 0;
                let height = 0;
                if (entry.kind === 'renderbuffer') {
                    const render = _webglRenderbufferState.get(entry.object);
                    if (!render || render.deleted) return WebGLRenderingContext.FRAMEBUFFER_INCOMPLETE_ATTACHMENT;
                    width = render.width;
                    height = render.height;
                } else {
                    const texture = _webglTextureState.get(entry.object);
                    if (!texture || texture.deleted) return WebGLRenderingContext.FRAMEBUFFER_INCOMPLETE_ATTACHMENT;
                    const image = texture.images.get(`${entry.target}:${entry.level}`);
                    if (!image) return WebGLRenderingContext.FRAMEBUFFER_INCOMPLETE_ATTACHMENT;
                    width = image.width;
                    height = image.height;
                }
                if (width <= 0 || height <= 0) return WebGLRenderingContext.FRAMEBUFFER_INCOMPLETE_ATTACHMENT;
                if (dimensions && (dimensions[0] !== width || dimensions[1] !== height)) {
                    return WebGLRenderingContext.FRAMEBUFFER_INCOMPLETE_DIMENSIONS;
                }
                dimensions = [width, height];
            }
            return WebGLRenderingContext.FRAMEBUFFER_COMPLETE;
        }
        isFramebuffer(framebuffer) {
            const state = _requireWebGLObject('WebGLFramebuffer', _webglFramebufferState, framebuffer, false, 'isFramebuffer');
            return !!state && state.context === this && !state.deleted && state.everBound;
        }
        isRenderbuffer(renderbuffer) {
            const state = _requireWebGLObject('WebGLRenderbuffer', _webglRenderbufferState, renderbuffer, false, 'isRenderbuffer');
            return !!state && state.context === this && !state.deleted && state.everBound;
        }
        enable() {}
        disable() {}
        blendFunc() {}
        blendEquation() {}
        depthFunc() {}
        depthMask() {}
        colorMask() {}
        scissor() {}
        pixelStorei() {}
        getError() {
            const error = _webglContextErrors.get(this) || WebGLRenderingContext.NO_ERROR;
            _webglContextErrors.set(this, WebGLRenderingContext.NO_ERROR);
            return error;
        }
        flush() {}
        finish() {}
        deleteShader(shader) {
            if (shader == null) return;
            const state = _requireWebGLObject('WebGLShader', _webglShaderState, shader);
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            state.deleted = true;
        }
        deleteProgram(program) {
            if (program == null) return;
            const state = _requireWebGLObject('WebGLProgram', _webglProgramState, program);
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            state.deleted = true;
        }
        deleteBuffer(buffer) {
            if (buffer == null) return;
            const state = _requireWebGLObject('WebGLBuffer', _webglBufferState, buffer, false, 'deleteBuffer');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            const bindings = _webglBindings(this);
            if (bindings.array === buffer) bindings.array = null;
            if (bindings.element === buffer) bindings.element = null;
        }
        deleteTexture(texture) {
            if (texture == null) return;
            const state = _requireWebGLObject('WebGLTexture', _webglTextureState, texture, false, 'deleteTexture');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            const bindings = _webglTextureBindingState(this);
            for (const unit of bindings.units) {
                if (unit.twoD === texture) unit.twoD = null;
                if (unit.cube === texture) unit.cube = null;
            }
            _webglDetachObjectFromFramebuffers(this, texture);
        }
        deleteFramebuffer(framebuffer) {
            if (framebuffer == null) return;
            const state = _requireWebGLObject('WebGLFramebuffer', _webglFramebufferState, framebuffer, false, 'deleteFramebuffer');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            state.attachments.clear();
            if (_webglFramebufferBinding.get(this) === framebuffer) _webglFramebufferBinding.set(this, null);
            _webglFramebuffers(this).delete(framebuffer);
        }
        deleteRenderbuffer(renderbuffer) {
            if (renderbuffer == null) return;
            const state = _requireWebGLObject('WebGLRenderbuffer', _webglRenderbufferState, renderbuffer, false, 'deleteRenderbuffer');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            if (_webglRenderbufferBinding.get(this) === renderbuffer) _webglRenderbufferBinding.set(this, null);
            _webglDetachObjectFromFramebuffers(this, renderbuffer);
        }
        isContextLost() { return false; }
    }

    // FIX-D2: WebGL2RenderingContext is a SEPARATE constructor from
    // WebGLRenderingContext (real Chrome: `WebGLRenderingContext !==
    // WebGL2RenderingContext`, and a webgl2 ctx has its own constructor +
    // "[object WebGL2RenderingContext]" tag). Pre-FIX-D2 we aliased the two,
    // so `WebGLRenderingContext === WebGL2RenderingContext` was a one-line bot
    // tell. The class shares all method bodies via inheritance; instances
    // carry `_isWebGL2 = true` (set in getContext) so the surface selector
    // returns the WebGL 2 surface. Static `_g/_g1/_surfaceFor/_gpuCache*` are
    // inherited and resolve to the same shared caches.
    class WebGL2RenderingContext {
        // WebGL2 is a distinct WebIDL interface in Blink. In particular,
        // `webgl2Context instanceof WebGLRenderingContext` is false and both
        // interface prototypes inherit directly from Object.prototype. Shared
        // WebGL1 operations are copied onto this prototype below rather than
        // represented as JavaScript inheritance.
        constructor(canvasId, width, height) {
            _configureWebGLContext(this, {
                canvasId,
                width: width === undefined ? 300 : width,
                height: height === undefined ? 150 : height,
                isWebGL2: true,
            });
        }

        // WebGL2 lifecycle/query constants used by the APIs below.
        static VERTEX_ARRAY_BINDING = 0x85B5;
        static CURRENT_QUERY = 0x8865;
        static QUERY_RESULT = 0x8866;
        static QUERY_RESULT_AVAILABLE = 0x8867;
        static ANY_SAMPLES_PASSED = 0x8C2F;
        static ANY_SAMPLES_PASSED_CONSERVATIVE = 0x8D6A;
        static TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN = 0x8C88;
        static SAMPLER_BINDING = 0x8919;
        static TEXTURE_WRAP_R = 0x8072;
        static TEXTURE_MIN_LOD = 0x813A;
        static TEXTURE_MAX_LOD = 0x813B;
        static TEXTURE_COMPARE_MODE = 0x884C;
        static TEXTURE_COMPARE_FUNC = 0x884D;
        static TRANSFORM_FEEDBACK = 0x8E22;
        static TRANSFORM_FEEDBACK_PAUSED = 0x8E23;
        static TRANSFORM_FEEDBACK_ACTIVE = 0x8E24;
        static TRANSFORM_FEEDBACK_BINDING = 0x8E25;
        static MAX_SERVER_WAIT_TIMEOUT = 0x9111;
        static OBJECT_TYPE = 0x9112;
        static SYNC_CONDITION = 0x9113;
        static SYNC_STATUS = 0x9114;
        static SYNC_FLAGS = 0x9115;
        static SYNC_FENCE = 0x9116;
        static SYNC_GPU_COMMANDS_COMPLETE = 0x9117;
        static UNSIGNALED = 0x9118;
        static SIGNALED = 0x9119;
        static ALREADY_SIGNALED = 0x911A;
        static TIMEOUT_EXPIRED = 0x911B;
        static CONDITION_SATISFIED = 0x911C;
        static WAIT_FAILED = 0x911D;
        static SYNC_FLUSH_COMMANDS_BIT = 0x00000001;
        static TIMEOUT_IGNORED = -1;
        static MAX_CLIENT_WAIT_TIMEOUT_WEBGL = 0x9247;

        getParameter(pname) {
            pname = Number(pname) >>> 0;
            if (pname === WebGL2RenderingContext.VERTEX_ARRAY_BINDING) {
                return _webglVertexArrayBinding.get(this) || null;
            }
            if (pname === WebGL2RenderingContext.SAMPLER_BINDING) {
                const unit = _webglTextureBindingState(this).activeUnit;
                return _webgl2SamplerBindingsFor(this).get(unit) || null;
            }
            if (pname === WebGL2RenderingContext.TRANSFORM_FEEDBACK_BINDING) {
                return _webglTransformFeedbackBinding.get(this) || null;
            }
            if (pname === WebGL2RenderingContext.TRANSFORM_FEEDBACK_ACTIVE ||
                pname === WebGL2RenderingContext.TRANSFORM_FEEDBACK_PAUSED) {
                const object = _webglTransformFeedbackBinding.get(this);
                const state = object && _webglTransformFeedbackState.get(object);
                if (pname === WebGL2RenderingContext.TRANSFORM_FEEDBACK_ACTIVE) return !!(state && state.active);
                return !!(state && state.paused);
            }
            return WebGLRenderingContext.prototype.getParameter.call(this, pname);
        }

        createVertexArray() {
            return _newWebGLObject('WebGLVertexArrayObject', _webglVertexArrayState, {
                context: this, deleted: false, everBound: false,
            });
        }
        bindVertexArray(vertexArray) {
            if (vertexArray == null) {
                _webglVertexArrayBinding.set(this, null);
                return;
            }
            const state = _requireWebGL2Object('WebGLVertexArrayObject', _webglVertexArrayState, vertexArray, false, 'bindVertexArray');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.everBound = true;
            _webglVertexArrayBinding.set(this, vertexArray);
        }
        deleteVertexArray(vertexArray) {
            if (vertexArray == null) return;
            const state = _requireWebGL2Object('WebGLVertexArrayObject', _webglVertexArrayState, vertexArray, false, 'deleteVertexArray');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            if (_webglVertexArrayBinding.get(this) === vertexArray) _webglVertexArrayBinding.set(this, null);
        }
        isVertexArray(vertexArray) {
            const state = _requireWebGL2Object('WebGLVertexArrayObject', _webglVertexArrayState, vertexArray, false, 'isVertexArray');
            return !!state && state.context === this && !state.deleted && state.everBound;
        }

        createQuery() {
            return _newWebGLObject('WebGLQuery', _webglQueryState, {
                context: this, deleted: false, everUsed: false, active: false,
                target: null, result: 0, available: false,
            });
        }
        beginQuery(target, query) {
            target = Number(target) >>> 0;
            const valid = target === WebGL2RenderingContext.ANY_SAMPLES_PASSED ||
                target === WebGL2RenderingContext.ANY_SAMPLES_PASSED_CONSERVATIVE ||
                target === WebGL2RenderingContext.TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN;
            if (!valid) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const state = _requireWebGL2Object('WebGLQuery', _webglQueryState, query, false, 'beginQuery');
            if (!state || state.context !== this || state.deleted || state.active || _webgl2QueryBindingsFor(this).get(target)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.everUsed = true;
            state.active = true;
            state.available = false;
            state.target = target;
            _webgl2QueryBindingsFor(this).set(target, query);
        }
        endQuery(target) {
            target = Number(target) >>> 0;
            const bindings = _webgl2QueryBindingsFor(this);
            const query = bindings.get(target);
            const state = query && _webglQueryState.get(query);
            if (!state || !state.active) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.active = false;
            // GPU query completion is asynchronous. Chrome still reports
            // QUERY_RESULT_AVAILABLE=false immediately after endQuery() on a
            // fresh context; without a real GPU queue keep it pending rather
            // than claiming a result is already ready.
            state.available = false;
            state.result = 0;
            bindings.delete(target);
        }
        getQuery(target, pname) {
            target = Number(target) >>> 0;
            pname = Number(pname) >>> 0;
            if (pname !== WebGL2RenderingContext.CURRENT_QUERY) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            return _webgl2QueryBindingsFor(this).get(target) || null;
        }
        getQueryParameter(query, pname) {
            const state = _requireWebGL2Object('WebGLQuery', _webglQueryState, query, false, 'getQueryParameter');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return null;
            }
            pname = Number(pname) >>> 0;
            if (pname === WebGL2RenderingContext.QUERY_RESULT_AVAILABLE) return !!state.available;
            if (pname === WebGL2RenderingContext.QUERY_RESULT) return state.result;
            _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
            return null;
        }
        deleteQuery(query) {
            if (query == null) return;
            const state = _requireWebGL2Object('WebGLQuery', _webglQueryState, query, false, 'deleteQuery');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            if (state.active && state.target !== null) _webgl2QueryBindingsFor(this).delete(state.target);
            state.active = false;
            state.deleted = true;
        }
        isQuery(query) {
            const state = _requireWebGL2Object('WebGLQuery', _webglQueryState, query, false, 'isQuery');
            return !!state && state.context === this && !state.deleted && state.everUsed;
        }

        createSampler() {
            return _newWebGLObject('WebGLSampler', _webglSamplerState, {
                context: this,
                deleted: false,
                parameters: new Map([
                    [WebGLRenderingContext.TEXTURE_MIN_FILTER, WebGLRenderingContext.NEAREST_MIPMAP_LINEAR],
                    [WebGLRenderingContext.TEXTURE_MAG_FILTER, WebGLRenderingContext.LINEAR],
                    [WebGLRenderingContext.TEXTURE_WRAP_S, WebGLRenderingContext.REPEAT],
                    [WebGLRenderingContext.TEXTURE_WRAP_T, WebGLRenderingContext.REPEAT],
                    [WebGL2RenderingContext.TEXTURE_WRAP_R, WebGLRenderingContext.REPEAT],
                    [WebGL2RenderingContext.TEXTURE_MIN_LOD, -1000],
                    [WebGL2RenderingContext.TEXTURE_MAX_LOD, 1000],
                    [WebGL2RenderingContext.TEXTURE_COMPARE_MODE, 0],
                    [WebGL2RenderingContext.TEXTURE_COMPARE_FUNC, 0x0203],
                ]),
            });
        }
        bindSampler(unit, sampler) {
            unit = Number(unit) >>> 0;
            if (unit >= 32) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            if (sampler == null) {
                _webgl2SamplerBindingsFor(this).delete(unit);
                return;
            }
            const state = _requireWebGL2Object('WebGLSampler', _webglSamplerState, sampler, false, 'bindSampler');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            _webgl2SamplerBindingsFor(this).set(unit, sampler);
        }
        samplerParameteri(sampler, pname, param) {
            _setWebGL2SamplerParameter(this, sampler, pname, Number(param) | 0, 'samplerParameteri');
        }
        samplerParameterf(sampler, pname, param) {
            _setWebGL2SamplerParameter(this, sampler, pname, Number(param), 'samplerParameterf');
        }
        getSamplerParameter(sampler, pname) {
            const state = _requireWebGL2Object('WebGLSampler', _webglSamplerState, sampler, false, 'getSamplerParameter');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return null;
            }
            pname = Number(pname) >>> 0;
            if (!state.parameters.has(pname)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            return state.parameters.get(pname);
        }
        deleteSampler(sampler) {
            if (sampler == null) return;
            const state = _requireWebGL2Object('WebGLSampler', _webglSamplerState, sampler, false, 'deleteSampler');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            const bindings = _webgl2SamplerBindingsFor(this);
            for (const [unit, bound] of bindings) if (bound === sampler) bindings.delete(unit);
        }
        isSampler(sampler) {
            const state = _requireWebGL2Object('WebGLSampler', _webglSamplerState, sampler, false, 'isSampler');
            return !!state && state.context === this && !state.deleted;
        }

        createTransformFeedback() {
            return _newWebGLObject('WebGLTransformFeedback', _webglTransformFeedbackState, {
                context: this, deleted: false, everBound: false, active: false, paused: false, primitiveMode: null,
            });
        }
        bindTransformFeedback(target, transformFeedback) {
            target = Number(target) >>> 0;
            if (target !== WebGL2RenderingContext.TRANSFORM_FEEDBACK) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            if (transformFeedback == null) {
                _webglTransformFeedbackBinding.set(this, null);
                return;
            }
            const state = _requireWebGL2Object('WebGLTransformFeedback', _webglTransformFeedbackState, transformFeedback, false, 'bindTransformFeedback');
            if (!state || state.context !== this || state.deleted || state.active) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.everBound = true;
            _webglTransformFeedbackBinding.set(this, transformFeedback);
        }
        beginTransformFeedback(primitiveMode) {
            primitiveMode = Number(primitiveMode) >>> 0;
            if (![WebGLRenderingContext.POINTS, WebGLRenderingContext.LINES, WebGLRenderingContext.TRIANGLES].includes(primitiveMode)) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return;
            }
            const object = _webglTransformFeedbackBinding.get(this);
            const state = object && _webglTransformFeedbackState.get(object);
            const program = _webglCurrentProgram.get(this);
            const programState = program && _webglProgramState.get(program);
            // Blink rejects beginTransformFeedback when there is no current
            // successfully-linked program. Keep the object inactive in that
            // case (the common no-program probe) instead of fabricating an
            // active transform-feedback session.
            if (!state || state.deleted || state.active || !programState || !programState.linked) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.active = true;
            state.paused = false;
            state.primitiveMode = primitiveMode;
        }
        endTransformFeedback() {
            const object = _webglTransformFeedbackBinding.get(this);
            const state = object && _webglTransformFeedbackState.get(object);
            if (!state || !state.active) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.active = false;
            state.paused = false;
            state.primitiveMode = null;
        }
        pauseTransformFeedback() {
            const object = _webglTransformFeedbackBinding.get(this);
            const state = object && _webglTransformFeedbackState.get(object);
            if (!state || !state.active || state.paused) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.paused = true;
        }
        resumeTransformFeedback() {
            const object = _webglTransformFeedbackBinding.get(this);
            const state = object && _webglTransformFeedbackState.get(object);
            if (!state || !state.active || !state.paused) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.paused = false;
        }
        deleteTransformFeedback(transformFeedback) {
            if (transformFeedback == null) return;
            const state = _requireWebGL2Object('WebGLTransformFeedback', _webglTransformFeedbackState, transformFeedback, false, 'deleteTransformFeedback');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_OPERATION);
                return;
            }
            state.deleted = true;
            state.active = false;
            state.paused = false;
            if (_webglTransformFeedbackBinding.get(this) === transformFeedback) _webglTransformFeedbackBinding.set(this, null);
        }
        isTransformFeedback(transformFeedback) {
            const state = _requireWebGL2Object('WebGLTransformFeedback', _webglTransformFeedbackState, transformFeedback, false, 'isTransformFeedback');
            return !!state && state.context === this && !state.deleted && state.everBound;
        }

        fenceSync(condition, flags) {
            condition = Number(condition) >>> 0;
            flags = Number(flags) >>> 0;
            if (condition !== WebGL2RenderingContext.SYNC_GPU_COMMANDS_COMPLETE) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
                return null;
            }
            if (flags !== 0) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            return _newWebGLObject('WebGLSync', _webglSyncState, {
                context: this, deleted: false, condition, flags, status: WebGL2RenderingContext.UNSIGNALED,
            });
        }
        clientWaitSync(sync, flags, timeout) {
            const state = _requireWebGL2Object('WebGLSync', _webglSyncState, sync, false, 'clientWaitSync');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return WebGL2RenderingContext.WAIT_FAILED;
            }
            flags = Number(flags) >>> 0;
            timeout = Number(timeout);
            if ((flags & ~WebGL2RenderingContext.SYNC_FLUSH_COMMANDS_BIT) !== 0 || !Number.isFinite(timeout) || timeout < 0) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return WebGL2RenderingContext.WAIT_FAILED;
            }
            if (state.status === WebGL2RenderingContext.SIGNALED) return WebGL2RenderingContext.ALREADY_SIGNALED;
            // With no real GPU command queue, zero-time polling mirrors the
            // observable initial Chrome state. A positive bounded wait is
            // treated as the command queue reaching the fence.
            if (timeout === 0) return WebGL2RenderingContext.TIMEOUT_EXPIRED;
            state.status = WebGL2RenderingContext.SIGNALED;
            return WebGL2RenderingContext.CONDITION_SATISFIED;
        }
        waitSync(sync, flags, timeout) {
            const state = _requireWebGL2Object('WebGLSync', _webglSyncState, sync, false, 'waitSync');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            if ((Number(flags) >>> 0) !== 0 || Number(timeout) !== WebGL2RenderingContext.TIMEOUT_IGNORED) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            state.status = WebGL2RenderingContext.SIGNALED;
        }
        getSyncParameter(sync, pname) {
            const state = _requireWebGL2Object('WebGLSync', _webglSyncState, sync, false, 'getSyncParameter');
            if (!state || state.context !== this || state.deleted) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return null;
            }
            pname = Number(pname) >>> 0;
            if (pname === WebGL2RenderingContext.OBJECT_TYPE) return WebGL2RenderingContext.SYNC_FENCE;
            if (pname === WebGL2RenderingContext.SYNC_CONDITION) return state.condition;
            if (pname === WebGL2RenderingContext.SYNC_STATUS) return state.status;
            if (pname === WebGL2RenderingContext.SYNC_FLAGS) return state.flags;
            _setWebGLError(this, WebGLRenderingContext.INVALID_ENUM);
            return null;
        }
        deleteSync(sync) {
            if (sync == null) return;
            const state = _requireWebGL2Object('WebGLSync', _webglSyncState, sync, false, 'deleteSync');
            if (!state || state.context !== this) {
                _setWebGLError(this, WebGLRenderingContext.INVALID_VALUE);
                return;
            }
            state.deleted = true;
        }
        isSync(sync) {
            const state = _requireWebGL2Object('WebGLSync', _webglSyncState, sync, false, 'isSync');
            return !!state && state.context === this && !state.deleted;
        }

        getInternalformatParameter(target, internalformat, pname) {
            // WebGL 2 exposes this method only for RENDERBUFFER/SAMPLES.
            // Chrome returns an Int32Array, including an empty typed array for
            // valid formats with no supported multisample counts.  The
            // challenge queries RGB32I (0x8d8e), whose Chrome result is [].
            if (target !== 0x8D41 || pname !== 0x80A9) return null;
            const multisampled = new Set([
                0x8051, // RGB8
                0x8058, // RGBA8
                0x8229, // R8
                0x822B, // RG8
                0x81A5, // DEPTH_COMPONENT16
                0x88F0, // DEPTH24_STENCIL8
            ]);
            return new Int32Array(multisampled.has(internalformat) ? [4, 2] : []);
        }
    }

    // Blink exposes WebGLRenderingContext and WebGL2RenderingContext as
    // sibling WebIDL interfaces (both prototypes directly inherit Object),
    // while the shared operations/constants appear as own properties on each
    // interface. Reuse the implementation descriptors without introducing a
    // JavaScript inheritance relationship.
    for (const key of Reflect.ownKeys(WebGLRenderingContext.prototype)) {
        if (key === 'constructor' || key === Symbol.toStringTag ||
            Object.prototype.hasOwnProperty.call(WebGL2RenderingContext.prototype, key)) continue;
        Object.defineProperty(
            WebGL2RenderingContext.prototype,
            key,
            Object.getOwnPropertyDescriptor(WebGLRenderingContext.prototype, key),
        );
    }
    for (const key of Object.getOwnPropertyNames(WebGLRenderingContext)) {
        if (key === 'length' || key === 'name' || key === 'prototype' ||
            Object.prototype.hasOwnProperty.call(WebGL2RenderingContext, key)) continue;
        const descriptor = Object.getOwnPropertyDescriptor(WebGLRenderingContext, key);
        if (descriptor && typeof descriptor.value === 'number') {
            Object.defineProperty(WebGL2RenderingContext, key, descriptor);
        }
    }

    // JavaScript class syntax does not use Web IDL descriptor defaults.
    // Normalize the public WebGL surface after defining the implementation
    // classes: operations are enumerable prototype members, while interface
    // constants are enumerable but immutable/non-configurable on both the
    // interface object and its prototype. Context instances therefore inherit
    // constants instead of leaking a private copy of every GLenum.
    const _finalizeWebGLIDL = (Ctor) => {
        const proto = Ctor.prototype;
        for (const key of Reflect.ownKeys(proto)) {
            if (key === 'constructor' || key === Symbol.toStringTag) continue;
            const desc = Object.getOwnPropertyDescriptor(proto, key);
            if (desc && !desc.enumerable &&
                (typeof desc.value === 'function' || typeof desc.get === 'function' || typeof desc.set === 'function')) {
                Object.defineProperty(proto, key, { ...desc, enumerable: true });
            }
        }
        for (const key of Object.getOwnPropertyNames(Ctor)) {
            const desc = Object.getOwnPropertyDescriptor(Ctor, key);
            if (!desc || typeof desc.value !== 'number') continue;
            const constant = {
                value: desc.value,
                writable: false,
                enumerable: true,
                configurable: false,
            };
            Object.defineProperty(Ctor, key, constant);
            if (!Object.prototype.hasOwnProperty.call(proto, key)) {
                Object.defineProperty(proto, key, constant);
            }
        }
    };
    _finalizeWebGLIDL(WebGLRenderingContext);
    _finalizeWebGLIDL(WebGL2RenderingContext);

    // WebAudio. Keep the existing native-rendering fingerprint path, but model
    // the public objects as actual WebIDL objects. The old implementation put
    // nearly every implementation detail directly on instances (`_context`,
    // AudioParam objects, compressor fields, etc.), while Blink stores the
    // state internally and exposes the surface through prototype accessors.
    // Chrome 148 therefore reports no own keys for these objects. WeakMaps here
    // preserve that shape without changing the Rust audio rendering algorithm.
    const _audioSampleRate = (() => {
        try {
            const has = ops.op_has_stealth_profile && ops.op_has_stealth_profile();
            if (has) {
                const raw = ops.op_get_profile_value("audio_sample_rate");
                const v = parseInt(raw, 10);
                // Stealth profile validate() restricts this to
                // {44100, 48000, 96000, 192000}; we trust it here.
                if (Number.isInteger(v) && v > 0) return v;
            }
        } catch (_) {}
        return 44100;
    })();
    // Chromium's headless audio output uses a 256-frame render quantum:
    // 256 / 48kHz = 0.005333… and 256 / 44.1kHz = 0.005804988….
    // There is no physical output device, so outputLatency is exactly zero.
    const _audioBaseLatency = 256 / _audioSampleRate;
    const _audioOutputLatency = 0;
    const _audioContextState = new WeakMap();
    const _audioNodeState = new WeakMap();
    const _audioScheduledState = new WeakMap();
    const _audioParamState = new WeakMap();
    const _audioBufferState = new WeakMap();
    const _audioListenerState = new WeakMap();
    const _oscillatorState = new WeakMap();
    const _compressorState = new WeakMap();
    const _biquadState = new WeakMap();
    const _analyserState = new WeakMap();
    const _gainState = new WeakMap();
    const _offlineAudioState = new WeakMap();
    const _offlineAudioCompletionEventState = new WeakMap();
    const _BASE_CONTEXT_TOKEN = Symbol('BaseAudioContext internal');
    const _AUDIO_NODE_TOKEN = Symbol('AudioNode internal');
    const _SCHEDULED_NODE_TOKEN = Symbol('AudioScheduledSourceNode internal');
    const _AUDIO_PARAM_TOKEN = Symbol('AudioParam internal');
    const _AUDIO_LISTENER_TOKEN = Symbol('AudioListener internal');
    const _DESTINATION_NODE_TOKEN = Symbol('AudioDestinationNode internal');

    const _audioMask = (fn, name) => {
        if (typeof fn !== 'function') return fn;
        try {
            Object.defineProperty(fn, 'name', { value: name, configurable: true });
            if (typeof globalThis._maskFunction === 'function') globalThis._maskFunction(fn, name);
        } catch (_) {}
        return fn;
    };
    const _audioMethod = (prototype, name, fn, length) => {
        try { Object.defineProperty(fn, 'length', { value: length, configurable: true }); } catch (_) {}
        _audioMask(fn, name);
        Object.defineProperty(prototype, name, {
            value: fn, writable: true, enumerable: true, configurable: true,
        });
    };
    const _audioAccessor = (prototype, name, get, set) => {
        if (get) _audioMask(get, `get ${name}`);
        if (set) _audioMask(set, `set ${name}`);
        Object.defineProperty(prototype, name, {
            get, set, enumerable: true, configurable: true,
        });
    };
    const _audioFinalize = (Ctor, name, length) => {
        try { delete Ctor.prototype.constructor; } catch (_) {}
        Object.defineProperty(Ctor.prototype, 'constructor', {
            value: Ctor, writable: true, enumerable: false, configurable: true,
        });
        Object.defineProperty(Ctor.prototype, Symbol.toStringTag, {
            value: name, writable: false, enumerable: false, configurable: true,
        });
        try { Object.defineProperty(Ctor, 'length', { value: length, configurable: true }); } catch (_) {}
        _audioMask(Ctor, name);
    };
    const _audioIllegal = name => {
        throw new TypeError(`Failed to construct '${name}': Illegal constructor`);
    };
    const _audioRequireArg = (name, count, actual) => {
        if (actual < count) {
            throw new TypeError(`Failed to construct '${name}': ${count} argument required, but only ${actual} present.`);
        }
    };
    const _requireAudioContext = (value, name) => {
        if (!_audioContextState.has(value)) {
            throw new TypeError(`Failed to construct '${name}': parameter 1 is not of type 'BaseAudioContext'.`);
        }
        return value;
    };
    const _dispatchAudioStateChange = context => {
        const state = _audioContextState.get(context);
        if (!state) return;
        try { context.dispatchEvent(new Event('statechange')); } catch (_) {}
        if (typeof state.onstatechange === 'function') {
            try { state.onstatechange.call(context, new Event('statechange')); } catch (_) {}
        }
    };

    class AudioParam {
        constructor(token = undefined, initial = 0, options = {}) {
            if (token !== _AUDIO_PARAM_TOKEN) _audioIllegal('AudioParam');
            const value = Number(initial);
            _audioParamState.set(this, {
                value,
                defaultValue: options.defaultValue === undefined ? value : Number(options.defaultValue),
                minValue: options.minValue === undefined ? -3.4028234663852886e38 : Number(options.minValue),
                maxValue: options.maxValue === undefined ? 3.4028234663852886e38 : Number(options.maxValue),
                automationRate: options.automationRate || 'a-rate',
                setter: typeof options.setter === 'function' ? options.setter : null,
            });
        }
    }
    delete AudioParam.prototype.constructor;
    _audioAccessor(AudioParam.prototype, 'value', function() {
        const state = _audioParamState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.value;
    }, function(value) {
        const state = _audioParamState.get(this); if (!state) throw new TypeError('Illegal invocation');
        state.value = Number(value); if (state.setter) state.setter(state.value);
    });
    _audioAccessor(AudioParam.prototype, 'automationRate', function() {
        const state = _audioParamState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.automationRate;
    }, function(value) {
        const state = _audioParamState.get(this); if (!state) throw new TypeError('Illegal invocation');
        const rate = String(value); if (rate === 'a-rate' || rate === 'k-rate') state.automationRate = rate;
    });
    for (const key of ['defaultValue', 'minValue', 'maxValue']) {
        _audioAccessor(AudioParam.prototype, key, function() {
            const state = _audioParamState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state[key];
        });
    }
    _audioMethod(AudioParam.prototype, 'cancelAndHoldAtTime', function(cancelTime) { void cancelTime; return this; }, 1);
    _audioMethod(AudioParam.prototype, 'cancelScheduledValues', function(cancelTime) { void cancelTime; return this; }, 1);
    _audioMethod(AudioParam.prototype, 'exponentialRampToValueAtTime', function(value, endTime) { void endTime; this.value = value; return this; }, 2);
    _audioMethod(AudioParam.prototype, 'linearRampToValueAtTime', function(value, endTime) { void endTime; this.value = value; return this; }, 2);
    _audioMethod(AudioParam.prototype, 'setTargetAtTime', function(target, startTime, timeConstant) { void startTime; void timeConstant; this.value = target; return this; }, 3);
    _audioMethod(AudioParam.prototype, 'setValueAtTime', function(value, startTime) { void startTime; this.value = value; return this; }, 2);
    _audioMethod(AudioParam.prototype, 'setValueCurveAtTime', function(values, startTime, duration) {
        void startTime; void duration;
        if (values && values.length) this.value = values[values.length - 1];
        return this;
    }, 3);
    _audioFinalize(AudioParam, 'AudioParam', 0);

    const _makeAudioParam = (value, options = {}) => new AudioParam(_AUDIO_PARAM_TOKEN, value, options);

    class AudioNode extends EventTarget {
        constructor(token = undefined, context, options = {}) {
            super();
            if (token !== _AUDIO_NODE_TOKEN) _audioIllegal('AudioNode');
            _audioNodeState.set(this, {
                context,
                numberOfInputs: options.numberOfInputs === undefined ? 1 : options.numberOfInputs,
                numberOfOutputs: options.numberOfOutputs === undefined ? 1 : options.numberOfOutputs,
                channelCount: options.channelCount === undefined ? 2 : options.channelCount,
                channelCountMode: options.channelCountMode || 'max',
                channelInterpretation: options.channelInterpretation || 'speakers',
                connections: [],
            });
        }
    }
    delete AudioNode.prototype.constructor;
    _audioAccessor(AudioNode.prototype, 'context', function() {
        const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.context;
    });
    _audioAccessor(AudioNode.prototype, 'numberOfInputs', function() {
        const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.numberOfInputs;
    });
    _audioAccessor(AudioNode.prototype, 'numberOfOutputs', function() {
        const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.numberOfOutputs;
    });
    for (const key of ['channelCount', 'channelCountMode', 'channelInterpretation']) {
        _audioAccessor(AudioNode.prototype, key, function() {
            const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state[key];
        }, function(value) {
            const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation');
            state[key] = key === 'channelCount' ? Number(value) : String(value);
        });
    }
    _audioMethod(AudioNode.prototype, 'connect', function(destination, output = 0, input = 0) {
        const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation');
        state.connections.push({ destination, output, input });
        return destination;
    }, 1);
    _audioMethod(AudioNode.prototype, 'disconnect', function() {
        const state = _audioNodeState.get(this); if (!state) throw new TypeError('Illegal invocation'); state.connections.length = 0;
    }, 0);
    _audioFinalize(AudioNode, 'AudioNode', 0);

    class AudioScheduledSourceNode extends AudioNode {
        constructor(token = undefined, context, options = {}) {
            super(_AUDIO_NODE_TOKEN, context, options);
            if (token !== _SCHEDULED_NODE_TOKEN) _audioIllegal('AudioScheduledSourceNode');
            _audioScheduledState.set(this, { onended: null, started: false, stopped: false });
        }
    }
    delete AudioScheduledSourceNode.prototype.constructor;
    _audioAccessor(AudioScheduledSourceNode.prototype, 'onended', function() {
        const state = _audioScheduledState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.onended;
    }, function(value) {
        const state = _audioScheduledState.get(this); if (!state) throw new TypeError('Illegal invocation'); state.onended = typeof value === 'function' ? value : null;
    });
    _audioMethod(AudioScheduledSourceNode.prototype, 'start', function(when = 0) {
        void when; const state = _audioScheduledState.get(this); if (!state) throw new TypeError('Illegal invocation'); state.started = true;
    }, 0);
    _audioMethod(AudioScheduledSourceNode.prototype, 'stop', function(when = 0) {
        void when; const state = _audioScheduledState.get(this); if (!state) throw new TypeError('Illegal invocation'); state.stopped = true;
    }, 0);
    _audioFinalize(AudioScheduledSourceNode, 'AudioScheduledSourceNode', 0);

    class OscillatorNode extends AudioScheduledSourceNode {
        constructor(context, options = {}) {
            _audioRequireArg('OscillatorNode', 1, arguments.length);
            _requireAudioContext(context, 'OscillatorNode');
            super(_SCHEDULED_NODE_TOKEN, context, { numberOfInputs: 0, numberOfOutputs: 1 });
            const offline = _offlineAudioState.get(context);
            const frequency = options.frequency === undefined ? 440 : Number(options.frequency);
            const detune = options.detune === undefined ? 0 : Number(options.detune);
            const sampleRate = _audioContextState.get(context).sampleRate;
            _oscillatorState.set(this, {
                type: options.type || 'sine',
                frequency: _makeAudioParam(frequency, {
                    defaultValue: 440, minValue: -sampleRate / 2, maxValue: sampleRate / 2,
                    setter: value => { const state = _offlineAudioState.get(context); if (state) state.oscFreq = value; },
                }),
                detune: _makeAudioParam(detune, { defaultValue: 0 }),
            });
            if (offline) { offline.oscType = options.type || 'sine'; offline.oscFreq = frequency; }
        }
    }
    delete OscillatorNode.prototype.constructor;
    _audioAccessor(OscillatorNode.prototype, 'type', function() {
        const state = _oscillatorState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state.type;
    }, function(value) {
        const state = _oscillatorState.get(this); if (!state) throw new TypeError('Illegal invocation');
        state.type = String(value); const offline = _offlineAudioState.get(this.context); if (offline) offline.oscType = state.type;
    });
    _audioAccessor(OscillatorNode.prototype, 'frequency', function() { const s=_oscillatorState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.frequency; });
    _audioAccessor(OscillatorNode.prototype, 'detune', function() { const s=_oscillatorState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.detune; });
    _audioMethod(OscillatorNode.prototype, 'setPeriodicWave', function(periodicWave) { void periodicWave; }, 1);
    _audioFinalize(OscillatorNode, 'OscillatorNode', 1);

    class GainNode extends AudioNode {
        constructor(context, options = {}) {
            _audioRequireArg('GainNode', 1, arguments.length); _requireAudioContext(context, 'GainNode');
            super(_AUDIO_NODE_TOKEN, context);
            _gainState.set(this, { gain: _makeAudioParam(options.gain === undefined ? 1 : Number(options.gain), { defaultValue: 1 }) });
        }
    }
    delete GainNode.prototype.constructor;
    _audioAccessor(GainNode.prototype, 'gain', function() { const s=_gainState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.gain; });
    _audioFinalize(GainNode, 'GainNode', 1);

    class DynamicsCompressorNode extends AudioNode {
        constructor(context, options = {}) {
            _audioRequireArg('DynamicsCompressorNode', 1, arguments.length); _requireAudioContext(context, 'DynamicsCompressorNode');
            super(_AUDIO_NODE_TOKEN, context);
            const update = (key, value) => { const state = _offlineAudioState.get(context); if (state) state[key] = value; };
            _compressorState.set(this, {
                threshold: _makeAudioParam(options.threshold === undefined ? -24 : Number(options.threshold), { defaultValue: -24, setter: v => update('compThreshold', v) }),
                knee: _makeAudioParam(options.knee === undefined ? 30 : Number(options.knee), { defaultValue: 30, setter: v => update('compKnee', v) }),
                ratio: _makeAudioParam(options.ratio === undefined ? 12 : Number(options.ratio), { defaultValue: 12, setter: v => update('compRatio', v) }),
                attack: _makeAudioParam(options.attack === undefined ? Math.fround(0.003) : Number(options.attack), { defaultValue: Math.fround(0.003), setter: v => update('compAttack', v) }),
                release: _makeAudioParam(options.release === undefined ? 0.25 : Number(options.release), { defaultValue: 0.25, setter: v => update('compRelease', v) }),
                reduction: 0,
            });
        }
    }
    delete DynamicsCompressorNode.prototype.constructor;
    for (const key of ['threshold', 'knee', 'ratio', 'reduction', 'attack', 'release']) {
        _audioAccessor(DynamicsCompressorNode.prototype, key, function() {
            const state = _compressorState.get(this); if (!state) throw new TypeError('Illegal invocation'); return state[key];
        });
    }
    _audioFinalize(DynamicsCompressorNode, 'DynamicsCompressorNode', 1);

    class BiquadFilterNode extends AudioNode {
        constructor(context, options = {}) {
            _audioRequireArg('BiquadFilterNode', 1, arguments.length); _requireAudioContext(context, 'BiquadFilterNode');
            super(_AUDIO_NODE_TOKEN, context);
            _biquadState.set(this, {
                type: options.type || 'lowpass',
                frequency: _makeAudioParam(options.frequency === undefined ? 350 : Number(options.frequency), { defaultValue: 350 }),
                detune: _makeAudioParam(options.detune === undefined ? 0 : Number(options.detune), { defaultValue: 0 }),
                Q: _makeAudioParam(options.Q === undefined ? 1 : Number(options.Q), { defaultValue: 1 }),
                gain: _makeAudioParam(options.gain === undefined ? 0 : Number(options.gain), { defaultValue: 0 }),
            });
        }
    }
    delete BiquadFilterNode.prototype.constructor;
    _audioAccessor(BiquadFilterNode.prototype, 'type', function() { const s=_biquadState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.type; }, function(v) { const s=_biquadState.get(this); if(!s) throw new TypeError('Illegal invocation'); s.type=String(v); });
    for (const key of ['frequency', 'detune', 'Q', 'gain']) {
        _audioAccessor(BiquadFilterNode.prototype, key, function() { const s=_biquadState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s[key]; });
    }
    _audioMethod(BiquadFilterNode.prototype, 'getFrequencyResponse', function(freqArr, magOut, phaseOut) {
        const state = _biquadState.get(this); if (!state) throw new TypeError('Illegal invocation');
        if (!(freqArr instanceof Float32Array)) return;
        const _typeIds = { lowpass:0, highpass:1, bandpass:2, lowshelf:3, highshelf:4, peaking:5, notch:6, allpass:7 };
        const contextState = _audioContextState.get(this.context);
        const out = ops.op_audio_biquad_response(
            new Uint8Array(freqArr.buffer, freqArr.byteOffset, freqArr.byteLength),
            _typeIds[state.type] ?? 0,
            state.frequency.value, state.Q.value, state.gain.value,
            contextState ? contextState.sampleRate : 44100,
        );
        const result = new Float32Array(out.buffer, out.byteOffset, out.byteLength / 4);
        const n = freqArr.length;
        for (let i=0; i<Math.min(magOut.length,n); i++) magOut[i]=result[i];
        for (let i=0; i<Math.min(phaseOut.length,n); i++) phaseOut[i]=result[n+i];
    }, 3);
    _audioFinalize(BiquadFilterNode, 'BiquadFilterNode', 1);

    class AnalyserNode extends AudioNode {
        constructor(context, options = {}) {
            _audioRequireArg('AnalyserNode', 1, arguments.length); _requireAudioContext(context, 'AnalyserNode');
            super(_AUDIO_NODE_TOKEN, context);
            _analyserState.set(this, {
                fftSize: options.fftSize === undefined ? 2048 : Number(options.fftSize),
                smoothingTimeConstant: options.smoothingTimeConstant === undefined ? 0.8 : Number(options.smoothingTimeConstant),
                minDecibels: options.minDecibels === undefined ? -100 : Number(options.minDecibels),
                maxDecibels: options.maxDecibels === undefined ? -30 : Number(options.maxDecibels),
                timeDomain: null, prevFreq: null,
            });
        }
    }
    delete AnalyserNode.prototype.constructor;
    _audioAccessor(AnalyserNode.prototype, 'fftSize', function() { const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.fftSize; }, function(v) { const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation'); s.fftSize=Number(v); });
    _audioAccessor(AnalyserNode.prototype, 'frequencyBinCount', function() { const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s.fftSize/2; });
    for (const key of ['minDecibels', 'maxDecibels', 'smoothingTimeConstant']) {
        _audioAccessor(AnalyserNode.prototype, key, function() { const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation'); return s[key]; }, function(v) { const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation'); s[key]=Number(v); });
    }
    _audioMethod(AnalyserNode.prototype, 'getByteFrequencyData', function(arr) {
        const f=new Float32Array(this.frequencyBinCount); this.getFloatFrequencyData(f);
        const s=_analyserState.get(this), range=s.maxDecibels-s.minDecibels;
        for(let i=0;i<Math.min(arr.length,f.length);i++) arr[i]=Math.max(0,Math.min(255,Math.round(((f[i]-s.minDecibels)/range)*255)));
    }, 1);
    _audioMethod(AnalyserNode.prototype, 'getByteTimeDomainData', function(arr) {
        const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation');
        if(!s.timeDomain){arr.fill(128);return;} for(let i=0;i<Math.min(arr.length,s.timeDomain.length);i++) arr[i]=Math.max(0,Math.min(255,Math.round((s.timeDomain[i]+1)*127.5)));
    }, 1);
    _audioMethod(AnalyserNode.prototype, 'getFloatFrequencyData', function(arr) {
        const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation');
        if(!s.timeDomain||s.timeDomain.length<s.fftSize){for(let i=0;i<arr.length;i++)arr[i]=s.minDecibels;return;}
        const prev=s.prevFreq?new Uint8Array(s.prevFreq.buffer):new Uint8Array(0);
        const out=ops.op_audio_analyser_freq_data(new Uint8Array(s.timeDomain.buffer,0,s.fftSize*4),s.fftSize,Math.round(s.smoothingTimeConstant*100),prev);
        const result=new Float32Array(out.buffer,out.byteOffset,out.byteLength/4); for(let i=0;i<Math.min(arr.length,result.length);i++)arr[i]=result[i]; s.prevFreq=result.slice();
    }, 1);
    _audioMethod(AnalyserNode.prototype, 'getFloatTimeDomainData', function(arr) {
        const s=_analyserState.get(this); if(!s) throw new TypeError('Illegal invocation');
        if(!s.timeDomain){arr.fill(0);return;} for(let i=0;i<Math.min(arr.length,s.timeDomain.length);i++)arr[i]=s.timeDomain[i];
    }, 1);
    _audioFinalize(AnalyserNode, 'AnalyserNode', 1);

    class AudioDestinationNode extends AudioNode {
        constructor(token = undefined, context) {
            if (token !== _DESTINATION_NODE_TOKEN) _audioIllegal('AudioDestinationNode');
            super(_AUDIO_NODE_TOKEN, context, { numberOfInputs:1, numberOfOutputs:0, channelCount:2, channelCountMode:'explicit', channelInterpretation:'speakers' });
        }
    }
    delete AudioDestinationNode.prototype.constructor;
    _audioAccessor(AudioDestinationNode.prototype, 'maxChannelCount', function() { if(!_audioNodeState.has(this))throw new TypeError('Illegal invocation'); return 2; });
    _audioFinalize(AudioDestinationNode, 'AudioDestinationNode', 0);

    class AudioListener {
        constructor(token = undefined) {
            if (token !== _AUDIO_LISTENER_TOKEN) _audioIllegal('AudioListener');
            _audioListenerState.set(this, {
                positionX:_makeAudioParam(0), positionY:_makeAudioParam(0), positionZ:_makeAudioParam(0),
                forwardX:_makeAudioParam(0), forwardY:_makeAudioParam(0), forwardZ:_makeAudioParam(-1),
                upX:_makeAudioParam(0), upY:_makeAudioParam(1), upZ:_makeAudioParam(0),
            });
        }
    }
    delete AudioListener.prototype.constructor;
    for (const key of ['positionX','positionY','positionZ','forwardX','forwardY','forwardZ','upX','upY','upZ']) {
        _audioAccessor(AudioListener.prototype, key, function() { const s=_audioListenerState.get(this); if(!s)throw new TypeError('Illegal invocation'); return s[key]; });
    }
    _audioMethod(AudioListener.prototype, 'setOrientation', function(x,y,z,xUp,yUp,zUp) {
        const s=_audioListenerState.get(this); if(!s)throw new TypeError('Illegal invocation');
        s.forwardX.value=x;s.forwardY.value=y;s.forwardZ.value=z;s.upX.value=xUp;s.upY.value=yUp;s.upZ.value=zUp;
    }, 6);
    _audioMethod(AudioListener.prototype, 'setPosition', function(x,y,z) {
        const s=_audioListenerState.get(this); if(!s)throw new TypeError('Illegal invocation'); s.positionX.value=x;s.positionY.value=y;s.positionZ.value=z;
    }, 3);
    _audioFinalize(AudioListener, 'AudioListener', 0);

    class AudioBuffer {
        constructor(options) {
            _audioRequireArg('AudioBuffer', 1, arguments.length);
            options = options || {};
            const channels=Math.max(1,Number(options.numberOfChannels===undefined?1:options.numberOfChannels)|0);
            const length=Math.max(1,Number(options.length)|0);
            const sampleRate=Number(options.sampleRate);
            const data=Array.from({length:channels},()=>new Float32Array(length));
            _audioBufferState.set(this,{channels,length,sampleRate,data});
        }
    }
    delete AudioBuffer.prototype.constructor;
    _audioAccessor(AudioBuffer.prototype,'length',function(){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.length;});
    _audioAccessor(AudioBuffer.prototype,'duration',function(){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.length/s.sampleRate;});
    _audioAccessor(AudioBuffer.prototype,'sampleRate',function(){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.sampleRate;});
    _audioAccessor(AudioBuffer.prototype,'numberOfChannels',function(){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.channels;});
    _audioMethod(AudioBuffer.prototype,'copyFromChannel',function(destination,channelNumber,startInChannel=0){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');const src=s.data[channelNumber];if(!src)throw new DOMException('The channel index provided is greater than or equal to the maximum bound.','IndexSizeError');destination.set(src.subarray(Number(startInChannel)||0,(Number(startInChannel)||0)+destination.length));},2);
    _audioMethod(AudioBuffer.prototype,'copyToChannel',function(source,channelNumber,startInChannel=0){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');const dst=s.data[channelNumber];if(!dst)throw new DOMException('The channel index provided is greater than or equal to the maximum bound.','IndexSizeError');dst.set(source.subarray(0,Math.max(0,dst.length-(Number(startInChannel)||0))),Number(startInChannel)||0);},2);
    _audioMethod(AudioBuffer.prototype,'getChannelData',function(channel){const s=_audioBufferState.get(this);if(!s)throw new TypeError('Illegal invocation');const data=s.data[Number(channel)];if(!data)throw new DOMException('The channel index provided is greater than or equal to the maximum bound.','IndexSizeError');return data;},1);
    _audioFinalize(AudioBuffer,'AudioBuffer',1);

    const _makeGenericNode = (name, context, scheduled = false) => {
        let proto = scheduled ? AudioScheduledSourceNode.prototype : AudioNode.prototype;
        try {
            const C = globalThis[name];
            if (typeof C === 'function' && C.prototype) {
                const parentProto = scheduled ? AudioScheduledSourceNode.prototype : AudioNode.prototype;
                if (Object.getPrototypeOf(C.prototype) !== parentProto) Object.setPrototypeOf(C.prototype, parentProto);
                if (Object.getPrototypeOf(C) !== (scheduled ? AudioScheduledSourceNode : AudioNode)) Object.setPrototypeOf(C, scheduled ? AudioScheduledSourceNode : AudioNode);
                proto = C.prototype;
            }
        } catch (_) {}
        const node=Object.create(proto);
        _audioNodeState.set(node,{context,numberOfInputs:scheduled?0:1,numberOfOutputs:1,channelCount:2,channelCountMode:'max',channelInterpretation:'speakers',connections:[]});
        if(scheduled)_audioScheduledState.set(node,{onended:null,started:false,stopped:false});
        return node;
    };

    class BaseAudioContext extends EventTarget {
        constructor(token = undefined, options = {}) {
            super();
            if (token !== _BASE_CONTEXT_TOKEN) _audioIllegal('BaseAudioContext');
            const sampleRate=Number(options.sampleRate||_audioSampleRate);
            _audioContextState.set(this,{
                sampleRate,currentTime:0,state:'suspended',onstatechange:null,onerror:null,onsinkchange:null,sinkId:'',
                baseLatency:256/sampleRate,outputLatency:0,listener:null,destination:null,audioWorklet:null,playbackStats:{},
            });
            const state=_audioContextState.get(this);
            state.destination=new AudioDestinationNode(_DESTINATION_NODE_TOKEN,this);
            state.listener=new AudioListener(_AUDIO_LISTENER_TOKEN);
            state.audioWorklet=Object.create(globalThis.AudioWorklet?.prototype||Object.prototype);
        }
    }
    delete BaseAudioContext.prototype.constructor;
    for(const key of ['destination','sampleRate','currentTime','listener','state']) _audioAccessor(BaseAudioContext.prototype,key,function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s[key];});
    _audioAccessor(BaseAudioContext.prototype,'onstatechange',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.onstatechange;},function(v){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.onstatechange=typeof v==='function'?v:null;});
    _audioMethod(BaseAudioContext.prototype,'createAnalyser',function(){return new AnalyserNode(this);},0);
    _audioMethod(BaseAudioContext.prototype,'createBiquadFilter',function(){return new BiquadFilterNode(this);},0);
    _audioMethod(BaseAudioContext.prototype,'createBuffer',function(channels,length,sampleRate){return new AudioBuffer({numberOfChannels:channels,length,sampleRate});},3);
    _audioMethod(BaseAudioContext.prototype,'createBufferSource',function(){return _makeGenericNode('AudioBufferSourceNode',this,true);},0);
    _audioMethod(BaseAudioContext.prototype,'createChannelMerger',function(){return _makeGenericNode('ChannelMergerNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createChannelSplitter',function(){return _makeGenericNode('ChannelSplitterNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createConstantSource',function(){return _makeGenericNode('ConstantSourceNode',this,true);},0);
    _audioMethod(BaseAudioContext.prototype,'createConvolver',function(){return _makeGenericNode('ConvolverNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createDelay',function(){return _makeGenericNode('DelayNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createDynamicsCompressor',function(){return new DynamicsCompressorNode(this);},0);
    _audioMethod(BaseAudioContext.prototype,'createGain',function(){return new GainNode(this);},0);
    _audioMethod(BaseAudioContext.prototype,'createIIRFilter',function(feedforward,feedback){void feedforward;void feedback;return _makeGenericNode('IIRFilterNode',this);},2);
    _audioMethod(BaseAudioContext.prototype,'createOscillator',function(){return new OscillatorNode(this);},0);
    _audioMethod(BaseAudioContext.prototype,'createPanner',function(){return _makeGenericNode('PannerNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createPeriodicWave',function(real,imag){void real;void imag;const C=globalThis.PeriodicWave;return typeof C==='function'?Object.create(C.prototype):{};},2);
    _audioMethod(BaseAudioContext.prototype,'createScriptProcessor',function(){return _makeGenericNode('ScriptProcessorNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createStereoPanner',function(){return _makeGenericNode('StereoPannerNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'createWaveShaper',function(){return _makeGenericNode('WaveShaperNode',this);},0);
    _audioMethod(BaseAudioContext.prototype,'decodeAudioData',function(audioData,successCallback,errorCallback){
        void audioData;void errorCallback;
        const buffer=new AudioBuffer({numberOfChannels:1,length:1,sampleRate:this.sampleRate});
        if(typeof successCallback==='function')queueMicrotask(()=>successCallback(buffer));
        return Promise.resolve(buffer);
    },1);
    _audioFinalize(BaseAudioContext,'BaseAudioContext',0);
    // Chromium installs `audioWorklet` after the prototype constructor.
    // Reposition the toStringTag so Reflect.ownKeys follows the WebIDL order.
    try { delete BaseAudioContext.prototype[Symbol.toStringTag]; } catch (_) {}
    _audioAccessor(BaseAudioContext.prototype,'audioWorklet',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.audioWorklet;});
    Object.defineProperty(BaseAudioContext.prototype, Symbol.toStringTag, {
        value:'BaseAudioContext', writable:false, enumerable:false, configurable:true,
    });

    class AudioContext extends BaseAudioContext {
        constructor(options = undefined) { super(_BASE_CONTEXT_TOKEN,options||{}); }
    }
    delete AudioContext.prototype.constructor;
    _audioAccessor(AudioContext.prototype,'baseLatency',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.baseLatency;});
    _audioAccessor(AudioContext.prototype,'outputLatency',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.outputLatency;});
    _audioAccessor(AudioContext.prototype,'onerror',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.onerror;},function(v){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.onerror=typeof v==='function'?v:null;});
    _audioMethod(AudioContext.prototype,'close',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.state='closed';_dispatchAudioStateChange(this);return Promise.resolve();},0);
    _audioMethod(AudioContext.prototype,'createMediaElementSource',function(element){void element;return _makeGenericNode('MediaElementAudioSourceNode',this);},1);
    _audioMethod(AudioContext.prototype,'createMediaStreamDestination',function(){return _makeGenericNode('MediaStreamAudioDestinationNode',this);},0);
    _audioMethod(AudioContext.prototype,'createMediaStreamSource',function(stream){void stream;return _makeGenericNode('MediaStreamAudioSourceNode',this);},1);
    _audioMethod(AudioContext.prototype,'getOutputTimestamp',function(){return {contextTime:this.currentTime,performanceTime:performance.now()};},0);
    _audioMethod(AudioContext.prototype,'resume',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.state='running';_dispatchAudioStateChange(this);return Promise.resolve();},0);
    _audioMethod(AudioContext.prototype,'suspend',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.state='suspended';_dispatchAudioStateChange(this);return Promise.resolve();},0);
    _audioAccessor(AudioContext.prototype,'playbackStats',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.playbackStats;});
    _audioFinalize(AudioContext,'AudioContext',0);
    // sink selection members were added after the original AudioContext
    // constructor slot and therefore appear after it in Chromium's own-key order.
    try { delete AudioContext.prototype[Symbol.toStringTag]; } catch (_) {}
    _audioAccessor(AudioContext.prototype,'sinkId',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.sinkId;});
    _audioAccessor(AudioContext.prototype,'onsinkchange',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.onsinkchange;},function(v){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.onsinkchange=typeof v==='function'?v:null;});
    _audioMethod(AudioContext.prototype,'setSinkId',function(id){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.sinkId=String(id);return Promise.resolve();},1);
    Object.defineProperty(AudioContext.prototype, Symbol.toStringTag, {
        value:'AudioContext', writable:false, enumerable:false, configurable:true,
    });

    class OfflineAudioCompletionEvent extends Event {
        constructor(type, eventInitDict) {
            _audioRequireArg('OfflineAudioCompletionEvent',2,arguments.length);
            if (!eventInitDict || !_audioBufferState.has(eventInitDict.renderedBuffer)) {
                throw new TypeError("Failed to construct 'OfflineAudioCompletionEvent': Failed to read the 'renderedBuffer' property from 'OfflineAudioCompletionEventInit': The provided value is not of type 'AudioBuffer'.");
            }
            super(type, eventInitDict);
            _offlineAudioCompletionEventState.set(this,{renderedBuffer:eventInitDict.renderedBuffer});
        }
    }
    _audioAccessor(OfflineAudioCompletionEvent.prototype,'renderedBuffer',function(){
        const s=_offlineAudioCompletionEventState.get(this);
        if(!s)throw new TypeError('Illegal invocation');
        return s.renderedBuffer;
    });
    _audioFinalize(OfflineAudioCompletionEvent,'OfflineAudioCompletionEvent',2);

    class OfflineAudioContext extends BaseAudioContext {
        constructor(numberOfChannels, length = undefined, sampleRate = undefined) {
            _audioRequireArg('OfflineAudioContext',1,arguments.length);
            let channels=numberOfChannels;
            if(numberOfChannels&&typeof numberOfChannels==='object'){
                const opts=numberOfChannels;channels=opts.numberOfChannels;length=opts.length;sampleRate=opts.sampleRate;
            }
            channels=Math.max(1,Number(channels)||1);length=Math.max(1,Number(length)||_audioSampleRate);sampleRate=Number(sampleRate)||_audioSampleRate;
            super(_BASE_CONTEXT_TOKEN,{sampleRate});
            _offlineAudioState.set(this,{channels,length,oscType:'triangle',oscFreq:10000,compThreshold:-24,compKnee:30,compRatio:12,compAttack:Math.fround(0.003),compRelease:0.25,oncomplete:null});
        }
    }
    delete OfflineAudioContext.prototype.constructor;
    _audioAccessor(OfflineAudioContext.prototype,'oncomplete',function(){const s=_offlineAudioState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.oncomplete;},function(v){const s=_offlineAudioState.get(this);if(!s)throw new TypeError('Illegal invocation');s.oncomplete=typeof v==='function'?v:null;});
    _audioAccessor(OfflineAudioContext.prototype,'length',function(){const s=_offlineAudioState.get(this);if(!s)throw new TypeError('Illegal invocation');return s.length;});
    _audioMethod(OfflineAudioContext.prototype,'resume',function(){const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.state='running';return Promise.resolve();},0);
    _audioMethod(OfflineAudioContext.prototype,'startRendering',function(){
        const self=this, offline=_offlineAudioState.get(this), context=_audioContextState.get(this);
        if(!offline||!context)return Promise.reject(new TypeError('Illegal invocation'));
        context.state='running';
        return new Promise(resolve=>{
            if(globalThis.__browser_oxide_debug){try{const log=globalThis.__oxAsyncApiDiag||(globalThis.__oxAsyncApiDiag=[]);log.push({api:'OfflineAudioContext.startRendering',phase:'call',at:performance.now()});}catch(_) {}}
            const sr=context.sampleRate,len=offline.length,freq=offline.oscFreq,type=offline.oscType;
            const waveTypeId=type==='sine'?0:type==='square'?2:type==='sawtooth'?3:1;
            let seed=0;try{if(ops.op_has_stealth_profile&&ops.op_has_stealth_profile()){const raw=ops.op_get_profile_value('audio_seed');if(raw){try{seed=Number(BigInt.asIntN(32,BigInt(raw)));}catch(_){const parsed=parseInt(raw,10);if(!Number.isNaN(parsed))seed=parsed|0;}}}}catch(_){}
            let data;try{const bytes=ops.op_offline_audio_render(seed,sr|0,len|0,freq,waveTypeId,offline.compThreshold,offline.compKnee,offline.compRatio,offline.compAttack,offline.compRelease);data=new Float32Array(bytes.buffer,bytes.byteOffset,len);}catch(_){data=new Float32Array(len);}
            const buffer=new AudioBuffer({numberOfChannels:offline.channels,length:len,sampleRate:sr});
            for(let c=0;c<offline.channels;c++)buffer.getChannelData(c).set(data);
            context.currentTime=len/sr;context.state='closed';
            if(globalThis.__browser_oxide_debug){try{globalThis.__oxAsyncApiDiag.push({api:'OfflineAudioContext.startRendering',phase:'resolve',length:len,at:performance.now()});}catch(_) {}}
            // Rendering completion is asynchronous in browsers. Queue the
            // completion so code can install `oncomplete` / `complete`
            // listeners immediately after startRendering() returns. Dispatch
            // through EventTarget so both listener styles observe the same
            // event and the Promise resolves to the identical AudioBuffer.
            queueMicrotask(()=>{
                const event=new OfflineAudioCompletionEvent('complete',{renderedBuffer:buffer});
                try{self.dispatchEvent(event);}catch(_){}
                resolve(buffer);
            });
        });
    },0);
    _audioMethod(OfflineAudioContext.prototype,'suspend',function(suspendTime){void suspendTime;const s=_audioContextState.get(this);if(!s)throw new TypeError('Illegal invocation');s.state='suspended';return Promise.resolve();},1);
    _audioFinalize(OfflineAudioContext,'OfflineAudioContext',1);

    // Publish the functional constructors over the early interface placeholders.
    Object.assign(globalThis,{
        AudioContext,BaseAudioContext,OfflineAudioContext,OfflineAudioCompletionEvent,AudioNode,AudioScheduledSourceNode,
        OscillatorNode,AudioParam,DynamicsCompressorNode,BiquadFilterNode,AnalyserNode,
        AudioBuffer,AudioListener,AudioDestinationNode,GainNode,
    });

    // HTMLCanvasElement: getContext returns the right context
    class HTMLCanvasElement {
        #canvasId;
        #attrs;
        constructor(width = 300, height = 150) {
            width = Math.max(0, Number(width) >>> 0);
            height = Math.max(0, Number(height) >>> 0);
            this.#canvasId = ops.op_canvas_create(width, height, _getOsName(), _getCanvasSeed());
            this.#attrs = { width: String(width), height: String(height) };
            // Element base properties — fpCollect and bot.sannysoft expect these.
            // Use defineProperty because Element.prototype (which we chain into
            // at the bottom of this file) has tagName/nodeName/etc. as getters
            // with no setters — direct assignment would fail.
            Object.defineProperty(this, 'tagName', { value: 'CANVAS', configurable: true, writable: true });
            Object.defineProperty(this, 'nodeName', { value: 'CANVAS', configurable: true, writable: true });
            Object.defineProperty(this, 'nodeType', { value: 1, configurable: true, writable: true });
            Object.defineProperty(this, 'style', { value: { cssText: "" }, configurable: true, writable: true });
            Object.defineProperty(this, 'classList', {
                value: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
                configurable: true, writable: true,
            });
            Object.defineProperty(this, 'dataset', { value: {}, configurable: true, writable: true });
            Object.defineProperty(this, 'childNodes', { value: [], configurable: true, writable: true });
            Object.defineProperty(this, 'children', { value: [], configurable: true, writable: true });
        }
        // Attribute API — required by canvas fingerprinters that do
        // `canvas.setAttribute('width', 200)` before drawing.
        setAttribute(name, value) {
            name = String(name);
            if (name === "width" || name === "height") {
                const parsed = Number.parseInt(value, 10);
                const fallback = name === "width" ? 300 : 150;
                this.#attrs[name] = String(Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback);
                ops.op_canvas_resize(this.#canvasId, this.width, this.height);
            } else {
                this.#attrs[name] = String(value);
            }
        }
        getAttribute(name) { return this.#attrs[name] !== undefined ? this.#attrs[name] : null; }
        removeAttribute(name) { delete this.#attrs[name]; }
        hasAttribute(name) { return name in this.#attrs; }
        getContext(type) {
            if (type === "2d") return new CanvasRenderingContext2D(this.#canvasId);
            if (type === "webgl" || type === "webgl2" || type === "experimental-webgl") {
                // FIX-D2: webgl2 → WebGL2RenderingContext (distinct class +
                // WebGL 2 surface); webgl/experimental-webgl → WebGLRenderingContext
                // with the WebGL 1 surface (_isWebGL2 = false).
                const isV2 = (type === "webgl2");
                const gl = isV2
                    ? new WebGL2RenderingContext(this.#canvasId, this.width, this.height)
                    : new WebGLRenderingContext(this.#canvasId, this.width, this.height);
                return _configureWebGLContext(gl, {
                    canvasId: this.#canvasId,
                    width: this.width,
                    height: this.height,
                    canvas: this,
                    isWebGL2: isV2,
                    resetViewport: true,
                });
            }
            return null;
        }
        toDataURL(type) { return ops.op_canvas_to_data_url(this.#canvasId); }
        toBlob(cb, type) { cb(new Blob([this.toDataURL()])); }
        // Minimal Node API
        appendChild(child) { this.childNodes.push(child); return child; }
        removeChild(child) {
            const i = this.childNodes.indexOf(child);
            if (i >= 0) this.childNodes.splice(i, 1);
            return child;
        }
        addEventListener(type, listener, options) {
            // Inherit from Node -> EventTarget
            return super.addEventListener(type, listener, options);
        }
        removeEventListener(type, listener, options) {
            return super.removeEventListener(type, listener, options);
        }
        dispatchEvent(event) {
            return super.dispatchEvent(event);
        }
        // Clone / get bounding box — fingerprint probes may call these
        cloneNode() { return new HTMLCanvasElement(this.width, this.height); }
        getBoundingClientRect() {
            return { x: 0, y: 0, width: this.width, height: this.height, top: 0, left: 0, right: this.width, bottom: this.height };
        }
    }

    // Do NOT replace globalThis.HTMLCanvasElement — dom_bootstrap already
    // exposes it as a subclass of HTMLElement ← Element ← Node ← EventTarget.
    // Instead, chain our standalone canvas class's prototype to the dom
    // HTMLCanvasElement.prototype so `standalone instanceof HTMLCanvasElement`
    // returns true.
    //
    // Capture the DOM-side HTMLCanvasElement.prototype BEFORE the swap so we
    // can also install the lazy `_canvasId`-based methods (getContext,
    // toDataURL, ...) onto it. HTML-parsed <canvas> elements have THIS
    // prototype in their chain — not the standalone's — so without this
    // double install they would not see `getContext`. The lazy methods
    // installed further down work on both kinds of canvas (`_canvasId`
    // is initialised on demand via `_lazyInitCanvas`).
    let _domCanvasProto = null;
    if (globalThis.HTMLCanvasElement) {
        _domCanvasProto = globalThis.HTMLCanvasElement.prototype;
        // The standalone helper class is retained for internal legacy paths,
        // but it is no longer published or returned from DOM creation.
        Object.setPrototypeOf(HTMLCanvasElement.prototype, _domCanvasProto);
        Object.setPrototypeOf(HTMLCanvasElement, globalThis.HTMLCanvasElement);
    }
    globalThis.CanvasRenderingContext2D = CanvasRenderingContext2D;
    globalThis.WebGLRenderingContext = WebGLRenderingContext;
    // Symbol.toStringTag — some scripts check
    // Object.prototype.toString.call(ctx) which must return
    // "[object CanvasRenderingContext2D]" / "[object WebGLRenderingContext]"
    // (not "[object Object]"). Without this tag we show as a bot.
    try {
        Object.defineProperty(CanvasRenderingContext2D.prototype, Symbol.toStringTag, {
            value: "CanvasRenderingContext2D",
            configurable: true,
        });
        Object.defineProperty(WebGLRenderingContext.prototype, Symbol.toStringTag, {
            value: "WebGLRenderingContext",
            configurable: true,
        });
        // FIX-D2: WebGL2RenderingContext is its own class now — give it its own
        // toStringTag so `Object.prototype.toString.call(gl2)` returns
        // "[object WebGL2RenderingContext]" (own prop shadows the inherited one).
        Object.defineProperty(WebGL2RenderingContext.prototype, Symbol.toStringTag, {
            value: "WebGL2RenderingContext",
            configurable: true,
        });
        Object.defineProperty(WebGLRenderingContext.prototype, 'constructor', {
            value: WebGLRenderingContext,
            configurable: true,
            writable: true,
        });
        Object.defineProperty(WebGL2RenderingContext.prototype, 'constructor', {
            value: WebGL2RenderingContext,
            configurable: true,
            writable: true,
        });
        Object.defineProperty(CanvasRenderingContext2D.prototype, 'constructor', {
            value: CanvasRenderingContext2D,
            configurable: true,
            writable: true,
        });
    } catch {}
    globalThis.WebGL2RenderingContext = WebGL2RenderingContext;
    globalThis.AudioContext = AudioContext;
    globalThis.OfflineAudioContext = OfflineAudioContext;
    globalThis.BaseAudioContext = BaseAudioContext;
    // Symbol.toStringTag for audio contexts — some scripts probe these.
    try {
        Object.defineProperty(AudioContext.prototype, Symbol.toStringTag, {
            value: "AudioContext", configurable: true,
        });
        Object.defineProperty(OfflineAudioContext.prototype, Symbol.toStringTag, {
            value: "OfflineAudioContext", configurable: true,
        });
        Object.defineProperty(BaseAudioContext.prototype, Symbol.toStringTag, {
            value: "BaseAudioContext", configurable: true,
        });
    } catch {}

    // Install canvas-specific methods on `HTMLCanvasElement.prototype`
    // directly (NOT on Element.prototype). Real Chrome's DOM uses
    // WebIDL-generated bindings where `getContext` / `toDataURL` /
    // `toBlob` are own properties of HTMLCanvasElement.prototype with
    // brand-checking that throws `TypeError: Illegal invocation` when
    // called on a non-canvas `this`. Fingerprint probes check for
    // this via `Object.getOwnPropertyDescriptor(HTMLCanvasElement
    // .prototype, 'getContext')` and by calling methods with bogus
    // `this` to observe the error message.
    const _HTMLCanvasProto = globalThis.HTMLCanvasElement &&
        globalThis.HTMLCanvasElement.prototype;
    if (_HTMLCanvasProto) {
        // Brand-check helper: Chrome throws `TypeError: Illegal
        // invocation` with no stack-relevant info beyond the message.
        //
        // We accept either `tagName === "CANVAS"` (for HTML-parsed
        // canvases whose tag name is authoritative) or
        // `this instanceof HTMLCanvasElement` (for standalone
        // canvases from createElement whose constructor sets
        // tagName after assigning width/height). This matches the
        // shape probes fingerprinters actually run while allowing
        // partially-constructed canvases to pass the setter path.
        function _requireCanvas(self, methodName) {
            const ok =
                self &&
                (self.tagName === "CANVAS" ||
                    self instanceof globalThis.HTMLCanvasElement);
            if (!ok) {
                throw new TypeError(
                    "Failed to execute '" +
                        methodName +
                        "' on 'HTMLCanvasElement': Illegal invocation"
                );
            }
        }
        function _lazyInitCanvas(self) {
            let canvasId = _canvasElementIds.get(self);
            if (canvasId === undefined) {
                const w = parseInt(self.getAttribute && self.getAttribute("width")) || 300;
                const h = parseInt(self.getAttribute && self.getAttribute("height")) || 150;
                canvasId = ops.op_canvas_create(w, h, _getOsName(), _getCanvasSeed());
                _canvasElementIds.set(self, canvasId);
            }
            return canvasId;
        }

        Object.defineProperty(_HTMLCanvasProto, "getContext", {
            value: function getContext(type) {
                _requireCanvas(this, "getContext");
                const canvasId = _lazyInitCanvas(this);
                if (type === "2d") return new CanvasRenderingContext2D(canvasId);
                if (
                    type === "webgl" ||
                    type === "webgl2" ||
                    type === "experimental-webgl"
                ) {
                    const w = parseInt(this.getAttribute("width")) || 300;
                    const h = parseInt(this.getAttribute("height")) || 150;
                    // FIX-D2: distinct class + surface per requested version.
                    const isV2 = (type === "webgl2");
                    const gl = isV2
                        ? new WebGL2RenderingContext(canvasId, w, h)
                        : new WebGLRenderingContext(canvasId, w, h);
                    return _configureWebGLContext(gl, {
                        canvasId,
                        width: w,
                        height: h,
                        canvas: this,
                        isWebGL2: isV2,
                        resetViewport: true,
                    });
                }
                return null;
            },
            writable: true,
            configurable: true,
            enumerable: false,
        });

        Object.defineProperty(_HTMLCanvasProto, "toDataURL", {
            value: function toDataURL(_type) {
                _requireCanvas(this, "toDataURL");
                // Auto-allocate a canvas if none yet — real Chrome
                // serializes any HTMLCanvasElement, even one whose 2D
                // context was never requested. The result is a fully
                // transparent PNG of the element's width × height.
                let canvasId = _canvasElementIds.get(this);
                if (canvasId === undefined) {
                    try { canvasId = _lazyInitCanvas(this); } catch (_e) {}
                }
                if (canvasId === undefined) return "data:,";
                return ops.op_canvas_to_data_url(canvasId);
            },
            writable: true,
            configurable: true,
            enumerable: false,
        });

        Object.defineProperty(_HTMLCanvasProto, "toBlob", {
            value: function toBlob(cb, type) {
                _requireCanvas(this, "toBlob");
                if (typeof cb !== "function") {
                    throw new TypeError(
                        "Failed to execute 'toBlob' on 'HTMLCanvasElement': callback is not a function"
                    );
                }
                // Match Chrome: the callback fires asynchronously on
                // the next microtask, not synchronously.
                const canvasId = _canvasElementIds.get(this);
                const url = canvasId !== undefined ? ops.op_canvas_to_data_url(canvasId) : "data:,";
                queueMicrotask(() => {
                    try {
                        cb(new Blob([url], { type: type || "image/png" }));
                    } catch (_e) {}
                });
            },
            writable: true,
            configurable: true,
            enumerable: false,
        });

        // Note: `width` and `height` are deliberately NOT installed on
        // the prototype here. The standalone canvas class in this
        // bootstrap sets them as own instance properties in its
        // constructor before `tagName` is defined, so adding a
        // brand-checking prototype setter breaks construction. A
        // prototype-level width/height accessor would also collide
        // with HTML-parsed `<canvas>` elements whose `getAttribute`
        // path is already canonical. Leave them as instance props.
    }

    // OffscreenCanvas — real canvas-backed implementation.
    //
    // Replaces the minimal stub from window_bootstrap.js (which had
    // `getContext() → null`). With canvas_ext already wired in for
    // the main thread and an identical bootstrap loading in workers,
    // `new OffscreenCanvas(w, h).getContext('2d')` now returns a
    // functional CanvasRenderingContext2D backed by the same ops the
    // on-DOM `<canvas>` element uses — real fillRect, real text,
    // real toDataURL.
    //
    // Anti-fingerprint sites probe this path via
    // `const ctx = new OffscreenCanvas(w, h).getContext('2d'); ctx.fillText(...)`.
    const _offscreenCanvasState = new WeakMap();
    const _requireOffscreenCanvasState = (canvas) => {
        const state = _offscreenCanvasState.get(canvas);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    class RealOffscreenCanvas extends EventTarget {
        constructor(width, height) {
            super();
            _offscreenCanvasState.set(this, {
                width: Math.max(0, width | 0),
                height: Math.max(0, height | 0),
                canvasId: 0,
                context2d: null,
                glctx1: null,
                glctx2: null,
                oncontextlost: null,
                oncontextrestored: null,
            });
        }
        get width() { return _requireOffscreenCanvasState(this).width; }
        set width(value) {
            const state = _requireOffscreenCanvasState(this);
            state.width = Math.max(0, Number(value) >>> 0);
            if (state.canvasId) {
                ops.op_canvas_resize(state.canvasId, state.width, state.height);
            }
        }
        get height() { return _requireOffscreenCanvasState(this).height; }
        set height(value) {
            const state = _requireOffscreenCanvasState(this);
            state.height = Math.max(0, Number(value) >>> 0);
            if (state.canvasId) {
                ops.op_canvas_resize(state.canvasId, state.width, state.height);
            }
        }
        get oncontextlost() { return _requireOffscreenCanvasState(this).oncontextlost; }
        set oncontextlost(value) {
            _requireOffscreenCanvasState(this).oncontextlost =
                typeof value === 'function' ? value : null;
        }
        get oncontextrestored() { return _requireOffscreenCanvasState(this).oncontextrestored; }
        set oncontextrestored(value) {
            _requireOffscreenCanvasState(this).oncontextrestored =
                typeof value === 'function' ? value : null;
        }
        getContext(type) {
            const state = _requireOffscreenCanvasState(this);
            if (type === "2d") {
                if (!state.canvasId) {
                    state.canvasId = ops.op_canvas_create(
                        state.width, state.height, _getOsName(), _getCanvasSeed()
                    );
                }
                if (!state.context2d) {
                    state.context2d = new CanvasRenderingContext2D(state.canvasId);
                    state.context2d.canvas = this;
                }
                return state.context2d;
            }
            if (type === "webgl" || type === "webgl2" || type === "experimental-webgl") {
                // FP parity: a real OffscreenCanvas exposes WebGL. Some
                // fingerprint workers read webGLVendor/webGLRenderer via
                // `new OffscreenCanvas(1,1).getContext('webgl')` →
                // gl.getParameter(UNMASKED_VENDOR_WEBGL); returning null here
                // differed from real Chrome (the on-DOM <canvas> already
                // supports WebGL).
                // Back it with the same profile-spoofed context that <canvas>
                // getContext uses (canvas_bootstrap.js:1232-1234).
                if (!state.canvasId) {
                    state.canvasId = ops.op_canvas_create(
                        state.width, state.height, _getOsName(), _getCanvasSeed()
                    );
                }
                const key = (type === "webgl2") ? "glctx2" : "glctx1";
                if (!state[key]) {
                    const isV2 = (type === "webgl2");
                    const gl = isV2
                        ? new WebGL2RenderingContext(state.canvasId, state.width, state.height)
                        : new WebGLRenderingContext(state.canvasId, state.width, state.height);
                    state[key] = _configureWebGLContext(gl, {
                        canvasId: state.canvasId,
                        width: state.width,
                        height: state.height,
                        canvas: this,
                        isWebGL2: isV2,
                        resetViewport: true,
                    });
                }
                return state[key];
            }
            return null;
        }
        transferToImageBitmap() {
            const state = _requireOffscreenCanvasState(this);
            return _makeImageBitmap({
                canvasId: state.canvasId,
                width: state.width,
                height: state.height,
            });
        }
        async convertToBlob() {
            const state = _requireOffscreenCanvasState(this);
            const options = arguments[0];
            const type = (options && options.type) || "image/png";
            if (!state.canvasId) {
                return new Blob([], { type });
            }
            // toDataURL returns `data:<type>;base64,<data>` — strip
            // the prefix and decode to bytes for a real Blob body.
            const url = ops.op_canvas_to_data_url(state.canvasId);
            const comma = url.indexOf(",");
            if (comma < 0) return new Blob([], { type });
            const b64 = url.slice(comma + 1);
            const bin = typeof atob === "function" ? atob(b64) : "";
            const bytes = new Uint8Array(bin.length);
            for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
            return new Blob([bytes], { type });
        }
    }
    for (const name of [
        'width', 'height', 'oncontextlost', 'oncontextrestored',
        'getContext', 'transferToImageBitmap', 'convertToBlob',
    ]) {
        const descriptor = Object.getOwnPropertyDescriptor(RealOffscreenCanvas.prototype, name);
        if (descriptor) {
            descriptor.enumerable = true;
            Object.defineProperty(RealOffscreenCanvas.prototype, name, descriptor);
        }
    }
    Object.defineProperty(RealOffscreenCanvas.prototype, Symbol.toStringTag, {
        value: "OffscreenCanvas",
        configurable: true,
    });
    // Install as the canonical global — overwrites the window_bootstrap stub.
    globalThis.OffscreenCanvas = RealOffscreenCanvas;

    // Mask methods as native
    if (typeof _maskAsNative === 'function') {
        _maskAsNative(CanvasRenderingContext2D.prototype, 
            'fillRect', 'strokeRect', 'clearRect', 'beginPath', 'moveTo', 'lineTo',
            'fill', 'stroke', 'closePath', 'arc', 'arcTo', 'bezierCurveTo',
            'quadraticCurveTo', 'rect', 'fillText', 'strokeText', 'measureText',
            'save', 'restore', 'translate', 'rotate', 'scale', 'setTransform',
            'resetTransform', 'getTransform', 'createLinearGradient', 
            'createRadialGradient', 'createPattern', 'getImageData', 'putImageData',
            'drawImage', 'isPointInPath', 'isPointInStroke');
        
        _maskAsNative(
            RealOffscreenCanvas.prototype,
            'width', 'height', 'oncontextlost', 'oncontextrestored',
            'getContext', 'transferToImageBitmap', 'convertToBlob'
        );

        const _offscreenTransferred = new WeakSet();
        // HTMLCanvasElement.prototype.transferControlToOffscreen — Chrome
        // 69+ method that returns a new OffscreenCanvas bound to this
        // element. Commonly probed as a real-Chrome
        // signal. Spec: https://html.spec.whatwg.org/#dom-canvas-transfercontroltooffscreen
        if (_HTMLCanvasProto && typeof _HTMLCanvasProto.transferControlToOffscreen !== "function") {
            const _transferControlToOffscreen = function transferControlToOffscreen() {
                const ok = this && (this.tagName === "CANVAS" ||
                    this instanceof globalThis.HTMLCanvasElement);
                if (!ok) {
                    throw new TypeError(
                        "Failed to execute 'transferControlToOffscreen' on 'HTMLCanvasElement': Illegal invocation");
                }
                if (_offscreenTransferred.has(this)) {
                    throw new DOMException(
                        "Cannot transfer control from a canvas for more than one time.",
                        "InvalidStateError");
                }
                const w = this.width || 300;
                const h = this.height || 150;
                _offscreenTransferred.add(this);
                return new RealOffscreenCanvas(w, h);
            };
            Object.defineProperty(_HTMLCanvasProto, "transferControlToOffscreen", {
                value: _transferControlToOffscreen, configurable: true, writable: true,
            });
            try { _maskAsNative(_HTMLCanvasProto, 'transferControlToOffscreen'); } catch (_) {}
        }

        if (_HTMLCanvasProto) {
            _maskAsNative(_HTMLCanvasProto, 'getContext', 'toDataURL', 'toBlob');
        }

        // Mirror the lazy-init canvas methods onto the DOM-side
        // HTMLCanvasElement.prototype too. HTML-parsed <canvas> elements
        // returned by `document.getElementById(...)` have that prototype
        // in their chain — not the standalone one — so without this
        // mirror, `elem.getContext` is `undefined` on every parsed canvas.
        // The standalone methods read `this._canvasId` (initialised lazily
        // via `_lazyInitCanvas`), which works for both kinds of canvas.
        if (_domCanvasProto && _domCanvasProto !== _HTMLCanvasProto) {
            for (const name of ['getContext', 'toDataURL', 'toBlob', 'transferControlToOffscreen']) {
                const desc = Object.getOwnPropertyDescriptor(_HTMLCanvasProto, name);
                if (desc && !Object.getOwnPropertyDescriptor(_domCanvasProto, name)) {
                    Object.defineProperty(_domCanvasProto, name, desc);
                }
            }
        }

        if (globalThis.AudioContext) {
            _maskAsNative(AudioContext.prototype, 'createOscillator', 'createDynamicsCompressor', 'close', 'suspend', 'resume');
        }
        if (globalThis.OfflineAudioContext) {
            _maskAsNative(OfflineAudioContext.prototype, 'startRendering');
        }
        if (globalThis.BaseAudioContext) {
            _maskAsNative(BaseAudioContext.prototype, 'createOscillator', 'createDynamicsCompressor', 'createAnalyser', 'createGain', 'createBiquadFilter');
        }
        
        // Mask every own-function method on WebGL[2]RenderingContext.prototype.
        // Many scripts inspect Function.prototype.toString of
        // these methods, which must serialize as native code. Iterating
        // the prototype's own names is durable as the engine grows method
        // coverage — every new method gets masked automatically.
        const _maskAllProtoFns = (proto) => {
            if (!proto) return;
            const names = [];
            for (const n of Object.getOwnPropertyNames(proto)) {
                if (n === 'constructor') continue;
                const d = Object.getOwnPropertyDescriptor(proto, n);
                if (d && typeof d.value === 'function') names.push(n);
            }
            if (names.length) _maskAsNative(proto, ...names);
        };
        if (globalThis.WebGLRenderingContext) {
            _maskAllProtoFns(globalThis.WebGLRenderingContext.prototype);
        }
        if (globalThis.WebGL2RenderingContext) {
            _maskAllProtoFns(globalThis.WebGL2RenderingContext.prototype);
        }
    }
})(globalThis);
