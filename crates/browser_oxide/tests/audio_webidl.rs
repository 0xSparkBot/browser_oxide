use browser_oxide::Page;

#[tokio::test]
async fn chrome_148_webaudio_webidl_shape_and_defaults() {
    let profile = browser_oxide::stealth::presets::chrome_148_macos();
    let mut page = Page::from_html_with_url(
        "<!doctype html><html><body></body></html>",
        "https://example.test/audio-webidl",
        Some(profile),
    )
    .await
    .unwrap();

    let raw = page
        .evaluate(
            r#"
            (() => {
                const keyName = key => typeof key === 'symbol'
                    ? `@@${key.description || ''}`
                    : String(key);
                const expected = {
                    BaseAudioContext: ['destination','sampleRate','currentTime','listener','state','onstatechange','createAnalyser','createBiquadFilter','createBuffer','createBufferSource','createChannelMerger','createChannelSplitter','createConstantSource','createConvolver','createDelay','createDynamicsCompressor','createGain','createIIRFilter','createOscillator','createPanner','createPeriodicWave','createScriptProcessor','createStereoPanner','createWaveShaper','decodeAudioData','constructor','audioWorklet','@@Symbol.toStringTag'],
                    AudioContext: ['baseLatency','outputLatency','onerror','close','createMediaElementSource','createMediaStreamDestination','createMediaStreamSource','getOutputTimestamp','resume','suspend','playbackStats','constructor','sinkId','onsinkchange','setSinkId','@@Symbol.toStringTag'],
                    OfflineAudioContext: ['oncomplete','length','resume','startRendering','suspend','constructor','@@Symbol.toStringTag'],
                    AudioNode: ['context','numberOfInputs','numberOfOutputs','channelCount','channelCountMode','channelInterpretation','connect','disconnect','constructor','@@Symbol.toStringTag'],
                    AudioScheduledSourceNode: ['onended','start','stop','constructor','@@Symbol.toStringTag'],
                    OscillatorNode: ['type','frequency','detune','setPeriodicWave','constructor','@@Symbol.toStringTag'],
                    AudioParam: ['value','automationRate','defaultValue','minValue','maxValue','cancelAndHoldAtTime','cancelScheduledValues','exponentialRampToValueAtTime','linearRampToValueAtTime','setTargetAtTime','setValueAtTime','setValueCurveAtTime','constructor','@@Symbol.toStringTag'],
                    DynamicsCompressorNode: ['threshold','knee','ratio','reduction','attack','release','constructor','@@Symbol.toStringTag'],
                    BiquadFilterNode: ['type','frequency','detune','Q','gain','getFrequencyResponse','constructor','@@Symbol.toStringTag'],
                    AnalyserNode: ['fftSize','frequencyBinCount','minDecibels','maxDecibels','smoothingTimeConstant','getByteFrequencyData','getByteTimeDomainData','getFloatFrequencyData','getFloatTimeDomainData','constructor','@@Symbol.toStringTag'],
                    AudioBuffer: ['length','duration','sampleRate','numberOfChannels','copyFromChannel','copyToChannel','getChannelData','constructor','@@Symbol.toStringTag'],
                    AudioListener: ['positionX','positionY','positionZ','forwardX','forwardY','forwardZ','upX','upY','upZ','setOrientation','setPosition','constructor','@@Symbol.toStringTag'],
                    AudioDestinationNode: ['maxChannelCount','constructor','@@Symbol.toStringTag'],
                    GainNode: ['gain','constructor','@@Symbol.toStringTag'],
                };
                const surfaceMismatches = {};
                for (const [name, keys] of Object.entries(expected)) {
                    const actual = Reflect.ownKeys(globalThis[name].prototype).map(keyName);
                    if (JSON.stringify(actual) !== JSON.stringify(keys)) {
                        surfaceMismatches[name] = { actual, expected: keys };
                    }
                    for (const key of Reflect.ownKeys(globalThis[name].prototype)) {
                        if (key === 'constructor' || key === Symbol.toStringTag) continue;
                        const descriptor = Object.getOwnPropertyDescriptor(globalThis[name].prototype, key);
                        if (!descriptor || descriptor.enumerable !== true || descriptor.configurable !== true) {
                            surfaceMismatches[`${name}.${String(key)}.descriptor`] = descriptor || null;
                        }
                    }
                }

                const ctx = new AudioContext();
                const offline = new OfflineAudioContext(1, 32, 44100);
                const objects = {
                    AudioContext: ctx,
                    OfflineAudioContext: offline,
                    AudioListener: ctx.listener,
                    AudioDestinationNode: ctx.destination,
                    OscillatorNode: ctx.createOscillator(),
                    DynamicsCompressorNode: ctx.createDynamicsCompressor(),
                    BiquadFilterNode: ctx.createBiquadFilter(),
                    AnalyserNode: ctx.createAnalyser(),
                    GainNode: ctx.createGain(),
                };
                const instanceMismatches = {};
                for (const [name, value] of Object.entries(objects)) {
                    const own = Reflect.ownKeys(value).map(keyName);
                    const tag = Object.prototype.toString.call(value);
                    if (own.length || tag !== `[object ${name}]`) {
                        instanceMismatches[name] = { own, tag };
                    }
                }

                const capture = fn => {
                    try { fn(); return 'NO_THROW'; }
                    catch (error) { return `${error.name}:${error.message}`; }
                };
                const osc = objects.OscillatorNode;
                const comp = objects.DynamicsCompressorNode;
                const biquad = objects.BiquadFilterNode;
                const analyser = objects.AnalyserNode;
                const gain = objects.GainNode;
                const result = {
                    surfaceMismatches,
                    instanceMismatches,
                    parents: {
                        AudioNode: Object.getPrototypeOf(AudioNode.prototype).constructor.name,
                        AudioScheduledSourceNode: Object.getPrototypeOf(AudioScheduledSourceNode.prototype).constructor.name,
                        OscillatorNode: Object.getPrototypeOf(OscillatorNode.prototype).constructor.name,
                        GainNode: Object.getPrototypeOf(GainNode.prototype).constructor.name,
                        BaseAudioContext: Object.getPrototypeOf(BaseAudioContext.prototype).constructor.name,
                        AudioContext: Object.getPrototypeOf(AudioContext.prototype).constructor.name,
                    },
                    lengths: {
                        BaseAudioContext: BaseAudioContext.length,
                        AudioContext: AudioContext.length,
                        OfflineAudioContext: OfflineAudioContext.length,
                        AudioNode: AudioNode.length,
                        OscillatorNode: OscillatorNode.length,
                        AudioBuffer: AudioBuffer.length,
                        AudioParam: AudioParam.length,
                    },
                    illegal: {
                        BaseAudioContext: capture(() => new BaseAudioContext()),
                        AudioNode: capture(() => new AudioNode()),
                        AudioScheduledSourceNode: capture(() => new AudioScheduledSourceNode()),
                        AudioParam: capture(() => new AudioParam()),
                        AudioListener: capture(() => new AudioListener()),
                        AudioDestinationNode: capture(() => new AudioDestinationNode()),
                        OscillatorNodeNoArgs: capture(() => new OscillatorNode()),
                    },
                    defaults: {
                        sampleRate: ctx.sampleRate,
                        baseLatency: ctx.baseLatency,
                        outputLatency: ctx.outputLatency,
                        state: ctx.state,
                        oscillator: [osc.type, osc.frequency.value, osc.detune.value],
                        oscillatorParam: [osc.frequency.automationRate, osc.frequency.defaultValue, osc.frequency.minValue, osc.frequency.maxValue],
                        compressor: [comp.threshold.value, comp.knee.value, comp.ratio.value, comp.attack.value, comp.release.value, comp.reduction],
                        biquad: [biquad.type, biquad.frequency.value, biquad.detune.value, biquad.Q.value, biquad.gain.value],
                        analyser: [analyser.fftSize, analyser.frequencyBinCount, analyser.minDecibels, analyser.maxDecibels, analyser.smoothingTimeConstant],
                        gain: gain.gain.value,
                        destination: [ctx.destination.maxChannelCount, ctx.destination.numberOfInputs, ctx.destination.numberOfOutputs, ctx.destination.channelCount, ctx.destination.channelCountMode, ctx.destination.channelInterpretation],
                        listener: [ctx.listener.positionX.value, ctx.listener.positionY.value, ctx.listener.positionZ.value, ctx.listener.forwardX.value, ctx.listener.forwardY.value, ctx.listener.forwardZ.value, ctx.listener.upX.value, ctx.listener.upY.value, ctx.listener.upZ.value],
                    },
                };
                void ctx.close();
                return JSON.stringify(result);
            })()
            "#,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(value["surfaceMismatches"], serde_json::json!({}));
    assert_eq!(value["instanceMismatches"], serde_json::json!({}));
    assert_eq!(
        value["parents"],
        serde_json::json!({
            "AudioNode": "EventTarget",
            "AudioScheduledSourceNode": "AudioNode",
            "OscillatorNode": "AudioScheduledSourceNode",
            "GainNode": "AudioNode",
            "BaseAudioContext": "EventTarget",
            "AudioContext": "BaseAudioContext",
        })
    );
    assert_eq!(
        value["lengths"],
        serde_json::json!({
            "BaseAudioContext": 0,
            "AudioContext": 0,
            "OfflineAudioContext": 1,
            "AudioNode": 0,
            "OscillatorNode": 1,
            "AudioBuffer": 1,
            "AudioParam": 0,
        })
    );
    for name in [
        "BaseAudioContext",
        "AudioNode",
        "AudioScheduledSourceNode",
        "AudioParam",
        "AudioListener",
        "AudioDestinationNode",
    ] {
        assert!(
            value["illegal"][name]
                .as_str()
                .unwrap()
                .contains("Illegal constructor"),
            "{name} must be an illegal public constructor: {}",
            value["illegal"][name]
        );
    }
    assert!(value["illegal"]["OscillatorNodeNoArgs"]
        .as_str()
        .unwrap()
        .contains("1 argument required"));

    let sample_rate = value["defaults"]["sampleRate"].as_f64().unwrap();
    assert_eq!(sample_rate, 48_000.0);
    assert_eq!(
        value["defaults"]["baseLatency"].as_f64().unwrap(),
        256.0 / sample_rate
    );
    assert_eq!(value["defaults"]["outputLatency"], 0);
    assert_eq!(value["defaults"]["state"], "suspended");
    assert_eq!(
        value["defaults"]["oscillator"],
        serde_json::json!(["sine", 440, 0])
    );
    assert_eq!(
        value["defaults"]["oscillatorParam"],
        serde_json::json!(["a-rate", 440, -24_000, 24_000])
    );
    assert_eq!(
        value["defaults"]["compressor"],
        serde_json::json!([-24, 30, 12, 0.003000000026077032, 0.25, 0])
    );
    assert_eq!(
        value["defaults"]["biquad"],
        serde_json::json!(["lowpass", 350, 0, 1, 0])
    );
    assert_eq!(
        value["defaults"]["analyser"],
        serde_json::json!([2048, 1024, -100, -30, 0.8])
    );
    assert_eq!(value["defaults"]["gain"], 1);
    assert_eq!(
        value["defaults"]["destination"],
        serde_json::json!([2, 1, 0, 2, "explicit", "speakers"])
    );
    assert_eq!(
        value["defaults"]["listener"],
        serde_json::json!([0, 0, 0, 0, 0, -1, 0, 1, 0])
    );
}
