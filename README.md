# Mandelbrot set visualization

A cross-platform GPU-accelerated visualization of the Mandelbrot set focused on interactivity.

Since WebGPU is currently at a very early stage, some driver related issues are expected to happen. Make sure you
have the latest driver version installed if you experience any issues or visual glitches.

On the web, WebGPU is only supported in Chromium-based browser at the time of writing this. For up-to-date
information see https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API#browser_compatibility

![minibrot](https://raw.githubusercontent.com/Anfid/media/master/Minibrot.png)


## Build

### Native

`cargo build --release`


### Web

`wasm-pack build --target web`
