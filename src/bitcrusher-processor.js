class BitcrusherProcessor extends AudioWorkletProcessor {
  static get parameterDescriptors() {
    return [
      { name: 'bits', defaultValue: 8, minValue: 1, maxValue: 16 },
      { name: 'reduction', defaultValue: 1, minValue: 1, maxValue: 32 },
    ];
  }
  constructor() {
    super();
    this._lastSample = 0;
    this._counter = 0;
  }
  process(inputs, outputs, params) {
    const input = inputs[0];
    const output = outputs[0];
    if (!input.length) return true;
    const bits = params.bits[0] || 8;
    const reduction = Math.floor(params.reduction[0] || 1);
    const step = Math.pow(0.5, bits);
    for (let ch = 0; ch < input.length; ch++) {
      const inp = input[ch];
      const out = output[ch];
      for (let i = 0; i < inp.length; i++) {
        this._counter++;
        if (this._counter >= reduction) {
          this._lastSample = step * Math.floor(inp[i] / step + 0.5);
          this._counter = 0;
        }
        out[i] = this._lastSample;
      }
    }
    return true;
  }
}
registerProcessor('bitcrusher-processor', BitcrusherProcessor);
