// DOMException WebIDL implementation shared by Window, snapshot, and Worker realms.
// Keep state off the instance: Blink exposes name/message/code as prototype
// accessors and DOMException instances have no own properties.
(() => {
    const _mask = typeof globalThis._maskFunction === 'function'
        ? globalThis._maskFunction
        : (fn) => fn;

    const _codes = [
        ['INDEX_SIZE_ERR', 1, 'IndexSizeError'],
        ['DOMSTRING_SIZE_ERR', 2, 'DOMStringSizeError'],
        ['HIERARCHY_REQUEST_ERR', 3, 'HierarchyRequestError'],
        ['WRONG_DOCUMENT_ERR', 4, 'WrongDocumentError'],
        ['INVALID_CHARACTER_ERR', 5, 'InvalidCharacterError'],
        ['NO_DATA_ALLOWED_ERR', 6, 'NoDataAllowedError'],
        ['NO_MODIFICATION_ALLOWED_ERR', 7, 'NoModificationAllowedError'],
        ['NOT_FOUND_ERR', 8, 'NotFoundError'],
        ['NOT_SUPPORTED_ERR', 9, 'NotSupportedError'],
        ['INUSE_ATTRIBUTE_ERR', 10, 'InUseAttributeError'],
        ['INVALID_STATE_ERR', 11, 'InvalidStateError'],
        ['SYNTAX_ERR', 12, 'SyntaxError'],
        ['INVALID_MODIFICATION_ERR', 13, 'InvalidModificationError'],
        ['NAMESPACE_ERR', 14, 'NamespaceError'],
        ['INVALID_ACCESS_ERR', 15, 'InvalidAccessError'],
        ['VALIDATION_ERR', 16, 'ValidationError'],
        ['TYPE_MISMATCH_ERR', 17, 'TypeMismatchError'],
        ['SECURITY_ERR', 18, 'SecurityError'],
        ['NETWORK_ERR', 19, 'NetworkError'],
        ['ABORT_ERR', 20, 'AbortError'],
        ['URL_MISMATCH_ERR', 21, 'URLMismatchError'],
        ['QUOTA_EXCEEDED_ERR', 22, 'QuotaExceededError'],
        ['TIMEOUT_ERR', 23, 'TimeoutError'],
        ['INVALID_NODE_TYPE_ERR', 24, 'InvalidNodeTypeError'],
        ['DATA_CLONE_ERR', 25, 'DataCloneError'],
    ];
    // DOM Level 1/2 kept three numeric constants for compatibility, but the
    // corresponding exception names are obsolete. Chromium still exposes the
    // constants (2/6/16) while `new DOMException('', name).code` returns 0.
    const _obsoleteCodeNames = new Set([
        'DOMStringSizeError',
        'NoDataAllowedError',
        'ValidationError',
    ]);
    const _codeByName = new Map(
        _codes
            .filter(([, , name]) => !_obsoleteCodeNames.has(name))
            .map(([, code, name]) => [name, code]),
    );
    const _state = new WeakMap();

    function DOMException(message = '', name = 'Error') {
        if (!new.target) {
            throw new TypeError(
                "Failed to construct 'DOMException': Please use the 'new' operator, this DOM object constructor cannot be called as a function."
            );
        }
        _state.set(this, {
            message: String(message),
            name: String(name),
        });
    }

    const proto = DOMException.prototype;
    Object.setPrototypeOf(proto, Error.prototype);
    // Re-add constructor after the WebIDL members/constants so Reflect.ownKeys
    // follows Blink's installation order.
    delete proto.constructor;

    const _getState = (self) => {
        const state = _state.get(self);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const getCode = _mask(function code() {
        const state = _getState(this);
        return _codeByName.get(state.name) || 0;
    }, 'get code');
    const getName = _mask(function name() {
        return _getState(this).name;
    }, 'get name');
    const getMessage = _mask(function message() {
        return _getState(this).message;
    }, 'get message');

    Object.defineProperty(proto, 'code', {
        get: getCode,
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(proto, 'name', {
        get: getName,
        enumerable: true,
        configurable: true,
    });
    Object.defineProperty(proto, 'message', {
        get: getMessage,
        enumerable: true,
        configurable: true,
    });

    for (const [constant, value] of _codes) {
        const descriptor = {
            value,
            writable: false,
            enumerable: true,
            configurable: false,
        };
        Object.defineProperty(DOMException, constant, descriptor);
        Object.defineProperty(proto, constant, descriptor);
    }

    Object.defineProperty(proto, 'constructor', {
        value: DOMException,
        writable: true,
        enumerable: false,
        configurable: true,
    });
    Object.defineProperty(proto, Symbol.toStringTag, {
        value: 'DOMException',
        writable: false,
        enumerable: false,
        configurable: true,
    });
    _mask(DOMException, 'DOMException');

    Object.defineProperty(globalThis, 'DOMException', {
        value: DOMException,
        writable: true,
        enumerable: false,
        configurable: true,
    });
})();
