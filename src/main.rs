fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        pollster::block_on(mandelbrot::run());
    }
}
