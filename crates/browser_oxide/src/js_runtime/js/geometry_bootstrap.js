// Geometry Interfaces: DOMRect / DOMPoint / DOMMatrix families.
// Browser-visible instances keep all state out-of-band so Reflect.ownKeys()
// matches Chromium's WebIDL objects.
((globalThis) => {
    const rectState = new WeakMap();
    const pointState = new WeakMap();
    const matrixState = new WeakMap();

    const requireState = (map, value) => {
        const state = map.get(value);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const number = (value, fallback = 0) => value === undefined ? fallback : Number(value);
    const tag = (prototype, name) => Object.defineProperty(prototype, Symbol.toStringTag, {
        value: name, configurable: true,
    });
    const enumerable = (prototype, names) => {
        for (const name of names) {
            const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
            if (!descriptor) continue;
            descriptor.enumerable = true;
            Object.defineProperty(prototype, name, descriptor);
        }
    };
    const enumerableStatic = (Ctor, names) => {
        for (const name of names) {
            const descriptor = Object.getOwnPropertyDescriptor(Ctor, name);
            if (!descriptor) continue;
            descriptor.enumerable = true;
            Object.defineProperty(Ctor, name, descriptor);
        }
    };

    class DOMRectReadOnly {
        constructor() {
            rectState.set(this, {
                x: number(arguments[0]), y: number(arguments[1]),
                width: number(arguments[2]), height: number(arguments[3]),
            });
        }
        static fromRect() {
            const r = arguments[0] || {};
            return new DOMRectReadOnly(r.x, r.y, r.width, r.height);
        }
        get x() { return requireState(rectState, this).x; }
        get y() { return requireState(rectState, this).y; }
        get width() { return requireState(rectState, this).width; }
        get height() { return requireState(rectState, this).height; }
        get top() { const s = requireState(rectState, this); return Math.min(s.y, s.y + s.height); }
        get right() { const s = requireState(rectState, this); return Math.max(s.x, s.x + s.width); }
        get bottom() { const s = requireState(rectState, this); return Math.max(s.y, s.y + s.height); }
        get left() { const s = requireState(rectState, this); return Math.min(s.x, s.x + s.width); }
        toJSON() {
            return { x:this.x, y:this.y, width:this.width, height:this.height,
                top:this.top, right:this.right, bottom:this.bottom, left:this.left };
        }
    }
    class DOMRect extends DOMRectReadOnly {
        constructor() { super(...arguments); }
        static fromRect() {
            const r = arguments[0] || {};
            return new DOMRect(r.x, r.y, r.width, r.height);
        }
        get x() { return requireState(rectState, this).x; }
        set x(value) { requireState(rectState, this).x = Number(value); }
        get y() { return requireState(rectState, this).y; }
        set y(value) { requireState(rectState, this).y = Number(value); }
        get width() { return requireState(rectState, this).width; }
        set width(value) { requireState(rectState, this).width = Number(value); }
        get height() { return requireState(rectState, this).height; }
        set height(value) { requireState(rectState, this).height = Number(value); }
    }

    class DOMPointReadOnly {
        constructor() {
            pointState.set(this, {
                x:number(arguments[0]), y:number(arguments[1]),
                z:number(arguments[2]), w:number(arguments[3], 1),
            });
        }
        static fromPoint() {
            const p = arguments[0] || {};
            return new DOMPointReadOnly(p.x, p.y, p.z, p.w);
        }
        get x() { return requireState(pointState, this).x; }
        get y() { return requireState(pointState, this).y; }
        get z() { return requireState(pointState, this).z; }
        get w() { return requireState(pointState, this).w; }
        matrixTransform() {
            const p = requireState(pointState, this);
            const m = matrixValues(arguments[0]);
            return new DOMPoint(
                m[0]*p.x + m[4]*p.y + m[8]*p.z + m[12]*p.w,
                m[1]*p.x + m[5]*p.y + m[9]*p.z + m[13]*p.w,
                m[2]*p.x + m[6]*p.y + m[10]*p.z + m[14]*p.w,
                m[3]*p.x + m[7]*p.y + m[11]*p.z + m[15]*p.w,
            );
        }
        toJSON() { return { x:this.x, y:this.y, z:this.z, w:this.w }; }
    }
    class DOMPoint extends DOMPointReadOnly {
        constructor() { super(...arguments); }
        static fromPoint() {
            const p = arguments[0] || {};
            return new DOMPoint(p.x, p.y, p.z, p.w);
        }
        get x() { return requireState(pointState, this).x; }
        set x(value) { requireState(pointState, this).x = Number(value); }
        get y() { return requireState(pointState, this).y; }
        set y(value) { requireState(pointState, this).y = Number(value); }
        get z() { return requireState(pointState, this).z; }
        set z(value) { requireState(pointState, this).z = Number(value); }
        get w() { return requireState(pointState, this).w; }
        set w(value) { requireState(pointState, this).w = Number(value); }
    }

    const identity = () => [1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1];
    const multiply = (a, b) => {
        const out = new Array(16).fill(0);
        for (let col=0; col<4; col++) for (let row=0; row<4; row++) {
            for (let k=0; k<4; k++) out[col*4+row] += a[k*4+row] * b[col*4+k];
        }
        return out;
    };
    const translation = (x,y,z) => [1,0,0,0, 0,1,0,0, 0,0,1,0, x,y,z,1];
    const scaling = (x,y,z) => [x,0,0,0, 0,y,0,0, 0,0,z,0, 0,0,0,1];
    const axisRotation = (x,y,z,degrees) => {
        const len = Math.hypot(x,y,z);
        if (!len) return identity();
        x/=len; y/=len; z/=len;
        const r=degrees*Math.PI/180, c=Math.cos(r), s=Math.sin(r), t=1-c;
        return [
            t*x*x+c, t*x*y+s*z, t*x*z-s*y, 0,
            t*x*y-s*z, t*y*y+c, t*y*z+s*x, 0,
            t*x*z+s*y, t*y*z-s*x, t*z*z+c, 0,
            0,0,0,1,
        ];
    };
    const inverse = values => {
        const a = Array.from({length:4}, (_,r) => Array.from({length:8}, (_,c) =>
            c < 4 ? values[c*4+r] : (c-4 === r ? 1 : 0)));
        for (let c=0; c<4; c++) {
            let p=c;
            for (let r=c+1; r<4; r++) if (Math.abs(a[r][c]) > Math.abs(a[p][c])) p=r;
            if (Math.abs(a[p][c]) < 1e-15) return new Array(16).fill(NaN);
            [a[c],a[p]]=[a[p],a[c]];
            const d=a[c][c]; for (let j=0;j<8;j++) a[c][j]/=d;
            for (let r=0;r<4;r++) if (r!==c) {
                const f=a[r][c]; for (let j=0;j<8;j++) a[r][j]-=f*a[c][j];
            }
        }
        const out=new Array(16);
        for(let c=0;c<4;c++) for(let r=0;r<4;r++) out[c*4+r]=a[r][c+4];
        return out;
    };
    const parseString = value => {
        const text=String(value).trim();
        if (!text || text === 'none') return identity();
        let m=/^matrix\((.*)\)$/i.exec(text);
        if (m) {
            const v=m[1].split(',').map(Number);
            if(v.length===6&&v.every(Number.isFinite)) return [v[0],v[1],0,0,v[2],v[3],0,0,0,0,1,0,v[4],v[5],0,1];
        }
        m=/^matrix3d\((.*)\)$/i.exec(text);
        if(m){const v=m[1].split(',').map(Number);if(v.length===16&&v.every(Number.isFinite))return v;}
        throw new TypeError('Invalid matrix string');
    };
    const matrixValues = init => {
        if (init === undefined || init === null) return identity();
        if (matrixState.has(init)) return matrixState.get(init).slice();
        if (typeof init === 'string') return parseString(init);
        if (Array.isArray(init) || ArrayBuffer.isView(init)) {
            const v=Array.from(init, Number);
            if(v.length===6)return [v[0],v[1],0,0,v[2],v[3],0,0,0,0,1,0,v[4],v[5],0,1];
            if(v.length===16)return v;
            throw new TypeError('DOMMatrix sequence must contain 6 or 16 values');
        }
        if (typeof init === 'object') {
            const out=identity(); let touched=false;
            const names=['m11','m12','m13','m14','m21','m22','m23','m24','m31','m32','m33','m34','m41','m42','m43','m44'];
            for(let i=0;i<names.length;i++)if(init[names[i]]!==undefined){out[i]=Number(init[names[i]]);touched=true;}
            for(const [name,index] of [['a',0],['b',1],['c',4],['d',5],['e',12],['f',13]])if(init[name]!==undefined){out[index]=Number(init[name]);touched=true;}
            return touched?out:identity();
        }
        throw new TypeError('Invalid DOMMatrix initializer');
    };
    const makeMatrix = (Ctor, values) => {
        const object=Object.create(Ctor.prototype); matrixState.set(object, values.slice()); return object;
    };
    const is2D = m => m[2]===0&&m[3]===0&&m[6]===0&&m[7]===0&&m[8]===0&&m[9]===0&&m[10]===1&&m[11]===0&&m[14]===0&&m[15]===1;
    const isIdentity = m => { const id=identity(); return m.every((v,i)=>v===id[i]); };

    class DOMMatrixReadOnly {
        constructor() { matrixState.set(this, matrixValues(arguments[0])); }
        static fromFloat32Array(array) { return makeMatrix(DOMMatrixReadOnly, matrixValues(array)); }
        static fromFloat64Array(array) { return makeMatrix(DOMMatrixReadOnly, matrixValues(array)); }
        static fromMatrix() { return makeMatrix(DOMMatrixReadOnly, matrixValues(arguments[0])); }
        get is2D() { return is2D(requireState(matrixState, this)); }
        get isIdentity() { return isIdentity(requireState(matrixState, this)); }
        multiply() { return makeMatrix(DOMMatrix, multiply(requireState(matrixState,this), matrixValues(arguments[0]))); }
        translate() {
            return makeMatrix(DOMMatrix, multiply(requireState(matrixState,this), translation(number(arguments[0]),number(arguments[1]),number(arguments[2]))));
        }
        scale() {
            const sx=number(arguments[0],1), sy=arguments[1]===undefined?sx:Number(arguments[1]), sz=number(arguments[2],1);
            const ox=number(arguments[3]), oy=number(arguments[4]), oz=number(arguments[5]);
            let op=translation(ox,oy,oz); op=multiply(op, scaling(sx,sy,sz)); op=multiply(op, translation(-ox,-oy,-oz));
            return makeMatrix(DOMMatrix, multiply(requireState(matrixState,this), op));
        }
        scale3d() { const s=number(arguments[0],1); return this.scale(s,s,s,arguments[1],arguments[2],arguments[3]); }
        scaleNonUniform() { return this.scale(arguments[0], arguments[1]); }
        rotate() {
            let x=number(arguments[0]), y=arguments[1], z=arguments[2];
            if(y===undefined&&z===undefined){z=x;x=0;y=0;} y=number(y); z=number(z);
            let op=axisRotation(1,0,0,x); op=multiply(op,axisRotation(0,1,0,y)); op=multiply(op,axisRotation(0,0,1,z));
            return makeMatrix(DOMMatrix, multiply(requireState(matrixState,this), op));
        }
        rotateFromVector() {
            const angle=Math.atan2(number(arguments[1]),number(arguments[0]))*180/Math.PI;
            return makeMatrix(DOMMatrix,multiply(requireState(matrixState,this),axisRotation(0,0,1,angle)));
        }
        rotateAxisAngle() {
            const op=axisRotation(number(arguments[0]),number(arguments[1]),number(arguments[2]),number(arguments[3]));
            return makeMatrix(DOMMatrix,multiply(requireState(matrixState,this),op));
        }
        skewX() {
            const a=Math.tan(number(arguments[0])*Math.PI/180);
            return makeMatrix(DOMMatrix,multiply(requireState(matrixState,this),[1,0,0,0,a,1,0,0,0,0,1,0,0,0,0,1]));
        }
        skewY() {
            const a=Math.tan(number(arguments[0])*Math.PI/180);
            return makeMatrix(DOMMatrix,multiply(requireState(matrixState,this),[1,a,0,0,0,1,0,0,0,0,1,0,0,0,0,1]));
        }
        flipX() { return this.scale(-1,1); }
        flipY() { return this.scale(1,-1); }
        inverse() { return makeMatrix(DOMMatrix,inverse(requireState(matrixState,this))); }
        transformPoint() {
            const p=arguments[0] instanceof DOMPointReadOnly ? arguments[0] : DOMPointReadOnly.fromPoint(arguments[0]);
            return p.matrixTransform(this);
        }
        toFloat32Array() { return new Float32Array(requireState(matrixState,this)); }
        toFloat64Array() { return new Float64Array(requireState(matrixState,this)); }
        toString() {
            const m=requireState(matrixState,this);
            return is2D(m)
                ? `matrix(${[m[0],m[1],m[4],m[5],m[12],m[13]].join(', ')})`
                : `matrix3d(${m.join(', ')})`;
        }
        toJSON() {
            const m=requireState(matrixState,this);
            return {
                a:m[0],b:m[1],c:m[4],d:m[5],e:m[12],f:m[13],
                m11:m[0],m12:m[1],m13:m[2],m14:m[3],
                m21:m[4],m22:m[5],m23:m[6],m24:m[7],
                m31:m[8],m32:m[9],m33:m[10],m34:m[11],
                m41:m[12],m42:m[13],m43:m[14],m44:m[15],
                is2D:is2D(m),isIdentity:isIdentity(m),
            };
        }
    }
    class DOMMatrix extends DOMMatrixReadOnly {
        constructor() { super(...arguments); }
        static fromFloat32Array(array) { return makeMatrix(DOMMatrix,matrixValues(array)); }
        static fromFloat64Array(array) { return makeMatrix(DOMMatrix,matrixValues(array)); }
        static fromMatrix() { return makeMatrix(DOMMatrix,matrixValues(arguments[0])); }
        multiplySelf() { matrixState.set(this,multiply(requireState(matrixState,this),matrixValues(arguments[0]))); return this; }
        preMultiplySelf() { matrixState.set(this,multiply(matrixValues(arguments[0]),requireState(matrixState,this))); return this; }
        translateSelf() { matrixState.set(this,multiply(requireState(matrixState,this),translation(number(arguments[0]),number(arguments[1]),number(arguments[2])))); return this; }
        scaleSelf() {
            const sx=number(arguments[0],1),sy=arguments[1]===undefined?sx:Number(arguments[1]),sz=number(arguments[2],1);
            const ox=number(arguments[3]),oy=number(arguments[4]),oz=number(arguments[5]);
            let op=translation(ox,oy,oz);op=multiply(op,scaling(sx,sy,sz));op=multiply(op,translation(-ox,-oy,-oz));
            matrixState.set(this,multiply(requireState(matrixState,this),op)); return this;
        }
        scale3dSelf() { const s=number(arguments[0],1); return this.scaleSelf(s,s,s,arguments[1],arguments[2],arguments[3]); }
        rotateSelf() {
            let x=number(arguments[0]),y=arguments[1],z=arguments[2];if(y===undefined&&z===undefined){z=x;x=0;y=0;}y=number(y);z=number(z);
            let op=axisRotation(1,0,0,x);op=multiply(op,axisRotation(0,1,0,y));op=multiply(op,axisRotation(0,0,1,z));
            matrixState.set(this,multiply(requireState(matrixState,this),op)); return this;
        }
        rotateFromVectorSelf() { const a=Math.atan2(number(arguments[1]),number(arguments[0]))*180/Math.PI;matrixState.set(this,multiply(requireState(matrixState,this),axisRotation(0,0,1,a)));return this; }
        rotateAxisAngleSelf() { matrixState.set(this,multiply(requireState(matrixState,this),axisRotation(number(arguments[0]),number(arguments[1]),number(arguments[2]),number(arguments[3]))));return this; }
        skewXSelf() { const a=Math.tan(number(arguments[0])*Math.PI/180);matrixState.set(this,multiply(requireState(matrixState,this),[1,0,0,0,a,1,0,0,0,0,1,0,0,0,0,1]));return this; }
        skewYSelf() { const a=Math.tan(number(arguments[0])*Math.PI/180);matrixState.set(this,multiply(requireState(matrixState,this),[1,a,0,0,0,1,0,0,0,0,1,0,0,0,0,1]));return this; }
        invertSelf() { matrixState.set(this,inverse(requireState(matrixState,this))); return this; }
        setMatrixValue(transformList) { matrixState.set(this,parseString(transformList)); return this; }
    }

    const readonlyMatrixProps={a:0,b:1,c:4,d:5,e:12,f:13,m11:0,m12:1,m13:2,m14:3,m21:4,m22:5,m23:6,m24:7,m31:8,m32:9,m33:10,m34:11,m41:12,m42:13,m43:14,m44:15};
    for(const [name,index] of Object.entries(readonlyMatrixProps)) {
        Object.defineProperty(DOMMatrixReadOnly.prototype,name,{get:function(){return requireState(matrixState,this)[index];},enumerable:true,configurable:true});
        Object.defineProperty(DOMMatrix.prototype,name,{get:function(){return requireState(matrixState,this)[index];},set:function(v){requireState(matrixState,this)[index]=Number(v);},enumerable:true,configurable:true});
    }

    enumerable(DOMRectReadOnly.prototype,['x','y','width','height','top','right','bottom','left','toJSON']);
    enumerable(DOMRect.prototype,['x','y','width','height']);
    enumerable(DOMPointReadOnly.prototype,['x','y','z','w','matrixTransform','toJSON']);
    enumerable(DOMPoint.prototype,['x','y','z','w']);
    enumerable(DOMMatrixReadOnly.prototype,['is2D','isIdentity','multiply','translate','scale','scale3d','scaleNonUniform','rotate','rotateFromVector','rotateAxisAngle','skewX','skewY','flipX','flipY','inverse','transformPoint','toFloat32Array','toFloat64Array','toString','toJSON']);
    enumerable(DOMMatrix.prototype,['multiplySelf','preMultiplySelf','translateSelf','scaleSelf','scale3dSelf','rotateSelf','rotateFromVectorSelf','rotateAxisAngleSelf','skewXSelf','skewYSelf','invertSelf','setMatrixValue']);
    enumerableStatic(DOMRectReadOnly,['fromRect']); enumerableStatic(DOMRect,['fromRect']);
    enumerableStatic(DOMPointReadOnly,['fromPoint']); enumerableStatic(DOMPoint,['fromPoint']);
    enumerableStatic(DOMMatrixReadOnly,['fromFloat32Array','fromFloat64Array','fromMatrix']);
    enumerableStatic(DOMMatrix,['fromFloat32Array','fromFloat64Array','fromMatrix']);

    tag(DOMRectReadOnly.prototype,'DOMRectReadOnly'); tag(DOMRect.prototype,'DOMRect');
    tag(DOMPointReadOnly.prototype,'DOMPointReadOnly'); tag(DOMPoint.prototype,'DOMPoint');
    tag(DOMMatrixReadOnly.prototype,'DOMMatrixReadOnly'); tag(DOMMatrix.prototype,'DOMMatrix');

    globalThis.DOMRectReadOnly=DOMRectReadOnly; globalThis.DOMRect=DOMRect;
    globalThis.DOMPointReadOnly=DOMPointReadOnly; globalThis.DOMPoint=DOMPoint;
    globalThis.DOMMatrixReadOnly=DOMMatrixReadOnly; globalThis.DOMMatrix=DOMMatrix; globalThis.WebKitCSSMatrix=DOMMatrix;

    try {
        if(typeof _maskFunction==='function')for(const [Ctor,name] of [[DOMRectReadOnly,'DOMRectReadOnly'],[DOMRect,'DOMRect'],[DOMPointReadOnly,'DOMPointReadOnly'],[DOMPoint,'DOMPoint'],[DOMMatrixReadOnly,'DOMMatrixReadOnly'],[DOMMatrix,'DOMMatrix']])_maskFunction(Ctor,name);
        if(typeof _maskAsNative==='function') {
            _maskAsNative(DOMRectReadOnly,'fromRect');_maskAsNative(DOMRect,'fromRect');
            _maskAsNative(DOMPointReadOnly,'fromPoint');_maskAsNative(DOMPoint,'fromPoint');
            _maskAsNative(DOMMatrixReadOnly,'fromFloat32Array','fromFloat64Array','fromMatrix');
            _maskAsNative(DOMMatrix,'fromFloat32Array','fromFloat64Array','fromMatrix');
        }
    } catch (_) {}
})(globalThis);
