use crate::primitives::{Coordinates, Dimensions, Point};

#[derive(Debug, Clone)]
pub struct ViewState {
    dimensions: Dimensions,
    scale_factor: f64,
    coords: Coordinates,
    reset: bool,
}

fn default_coordinates(dimensions: Dimensions, scale_factor: f64, precision: usize) -> Coordinates {
    let step = 4.0 * scale_factor as f32 / dimensions.shortest_side() as f32;
    let x = -(dimensions.width as f32 / scale_factor as f32 / 2.0) * step;
    let y = -(dimensions.height as f32 / scale_factor as f32 / 2.0) * step;
    Coordinates::new(x, y, step, precision)
}

impl ViewState {
    pub fn default(dimensions: Dimensions, scale_factor: f64, precision: usize) -> Self {
        Self {
            dimensions,
            scale_factor,
            coords: default_coordinates(dimensions, scale_factor, precision),
            reset: true,
        }
    }

    pub fn reset(&mut self) {
        self.reset = true;
        self.coords = default_coordinates(self.dimensions, self.scale_factor, self.precision());
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    pub fn set_dimensions(&mut self, dimensions: Dimensions) {
        if self.reset {
            self.dimensions = dimensions;
            self.coords = default_coordinates(dimensions, self.scale_factor, self.precision())
        } else {
            self.dimensions = dimensions;
        }
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        if self.reset {
            self.scale_factor = scale_factor;
            self.coords = default_coordinates(self.dimensions, scale_factor, self.precision());
        } else {
            let mul = scale_factor / self.scale_factor;
            self.coords.step = &self.coords.step
                * &crate::float::WideFloat::from_f32(mul as f32, self.coords.size()).unwrap();
            self.scale_factor = scale_factor;
        }
    }

    pub fn coords(&self) -> &Coordinates {
        &self.coords
    }

    pub fn precision(&self) -> usize {
        self.coords.precision()
    }

    pub fn set_precision(&mut self, precision: usize) {
        self.coords.set_precision(precision)
    }

    pub fn zoom_with_anchor(&mut self, delta: f32, anchor: Option<Point>) {
        self.reset = false;
        let anchor = anchor.unwrap_or(Point {
            x: (self.dimensions.width / 2) as f32,
            y: (self.dimensions.height / 2) as f32,
        });

        let mul = if delta > 0.0 {
            1.0 / (1.0 + delta)
        } else {
            1.0 - delta
        };

        self.coords.zoom_with_anchor(
            mul,
            (anchor.x / self.scale_factor as f32).round() as i32,
            (anchor.y / self.scale_factor as f32).round() as i32,
            2.0 * 4.0 / self.dimensions.shortest_side() as f32 * self.scale_factor as f32,
        );

        log::info!(
            "x: {}, y: {}, scale: {}",
            self.coords.x.as_f32_round(),
            self.coords.y.as_f32_round(),
            self.coords.step.as_f32_round(),
        );
    }

    pub fn move_by_screen_delta(&mut self, dx: f32, dy: f32) {
        self.reset = false;
        self.coords
            .move_by_delta(dx / self.scale_factor as f32, dy / self.scale_factor as f32);

        log::info!(
            "x: {}, y: {}",
            self.coords.x.as_f32_round(),
            self.coords.y.as_f32_round(),
        );
    }
}
