//! Draw scaled font outlines into GD's pixel buffer with bounded coverage storage.

use ab_glyph_rasterizer::{Point, Rasterizer, point};
use skrifa::outline::OutlinePen;

const MAX_SEGMENTS: usize = 65_536;
const MAX_COORDINATE: f32 = 1_048_576.0;
const MAX_PIXELS: f32 = 1_048_576.0;

pub(super) struct Path {
    segments: Vec<Segment>,
    start: Point,
    cursor: Point,
    bounds: Option<[f32; 4]>,
    invalid: bool,
}

enum Segment {
    Line([Point; 2]),
    Quadratic([Point; 3]),
    Cubic([Point; 4]),
}

impl Path {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            start: point(0.0, 0.0),
            cursor: point(0.0, 0.0),
            bounds: None,
            invalid: false,
        }
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn bounds(&self) -> Result<Option<[f32; 4]>, &'static str> {
        if self.invalid {
            return Err("Glyph exceeds the rendering limit");
        }
        Ok(self.bounds)
    }

    fn push(&mut self, segment: Segment) {
        if self.segments.len() == MAX_SEGMENTS || self.invalid {
            self.invalid = true;
            return;
        }
        let points = match &segment {
            Segment::Line(points) => points.as_slice(),
            Segment::Quadratic(points) => points.as_slice(),
            Segment::Cubic(points) => points.as_slice(),
        };
        for p in points {
            if !p.x.is_finite()
                || !p.y.is_finite()
                || p.x.abs() > MAX_COORDINATE
                || p.y.abs() > MAX_COORDINATE
            {
                self.invalid = true;
                return;
            }
            let bounds = self.bounds.get_or_insert([p.x, p.y, p.x, p.y]);
            bounds[0] = bounds[0].min(p.x);
            bounds[1] = bounds[1].min(p.y);
            bounds[2] = bounds[2].max(p.x);
            bounds[3] = bounds[3].max(p.y);
        }
        self.segments.push(segment);
    }

    pub fn draw(&self, angle: f32, offset: Point, mut paint: impl FnMut(i64, i64, f32)) {
        let Some([left, top, right, bottom]) = self.bounds else {
            return;
        };
        let (sin, cos) = angle.sin_cos();
        let transform = |p: Point| {
            let p = p + offset;
            point(p.x * cos + p.y * sin, -p.x * sin + p.y * cos)
        };
        let points = [
            point(left, top),
            point(left, bottom),
            point(right, top),
            point(right, bottom),
        ]
        .map(transform);
        let start_x = points
            .iter()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min)
            .floor();
        let start_y = points
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min)
            .floor();
        let end_x = points
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil();
        let end_y = points
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil();
        let (width, height) = (end_x - start_x, end_y - start_y);
        if width <= 0.0 || height <= 0.0 || !(0.0..=MAX_PIXELS).contains(&(width * height)) {
            return;
        }
        let mut raster = Rasterizer::new(width as usize, height as usize);
        let relative = |p| transform(p) - point(start_x, start_y);
        for segment in &self.segments {
            match segment {
                Segment::Line(points) => {
                    let [a, b] = points.map(relative);
                    raster.draw_line(a, b);
                }
                Segment::Quadratic(points) => {
                    let [a, b, c] = points.map(relative);
                    raster.draw_quad(a, b, c);
                }
                Segment::Cubic(points) => {
                    let [a, b, c, d] = points.map(relative);
                    raster.draw_cubic(a, b, c, d);
                }
            }
        }
        raster.for_each_pixel_2d(|x, y, coverage| {
            paint(
                start_x as i64 + i64::from(x),
                start_y as i64 + i64::from(y),
                coverage,
            );
        });
    }
}

impl OutlinePen for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cursor = point(x, -y);
        self.start = self.cursor;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let end = point(x, -y);
        self.push(Segment::Line([self.cursor, end]));
        self.cursor = end;
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let end = point(x, -y);
        self.push(Segment::Quadratic([self.cursor, point(cx, -cy), end]));
        self.cursor = end;
    }

    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        let end = point(x, -y);
        self.push(Segment::Cubic([
            self.cursor,
            point(ax, -ay),
            point(bx, -by),
            end,
        ]));
        self.cursor = end;
    }

    fn close(&mut self) {
        if self.cursor != self.start {
            self.push(Segment::Line([self.cursor, self.start]));
        }
        self.cursor = self.start;
    }
}
