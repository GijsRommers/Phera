//! A GD image holds packed GD colors or indices, as returned by `imagecolorat($im, $x, $y)`.

pub const MAX_PIXELS: usize = 16_777_216;
pub const MAX_DIMENSION: u32 = 32_768;
pub const DEFAULT_DPI: u32 = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Self = Self(0);
    pub const TRANSPARENT: Self = Self(127 << 24);

    pub fn gd(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self(u32::from_be_bytes([alpha & 127, red, green, blue]))
    }

    pub fn components(self) -> [u8; 4] {
        let [alpha, red, green, blue] = self.0.to_be_bytes();
        [red, green, blue, alpha & 127]
    }

    pub fn from_rgba([red, green, blue, alpha]: [u8; 4]) -> Self {
        Self::gd(red, green, blue, 127 - (alpha >> 1))
    }

    pub fn rgba(self, save_alpha: bool) -> [u8; 4] {
        let [red, green, blue, alpha] = self.components();
        let alpha = if save_alpha {
            255 - (alpha << 1 | alpha >> 6)
        } else {
            255
        };
        [red, green, blue, alpha]
    }

    fn over(self, dst: Self) -> Self {
        let [red, green, blue, alpha] = self.components().map(u32::from);
        let [dst_red, dst_green, dst_blue, dst_alpha] = dst.components().map(u32::from);
        match (alpha, dst_alpha) {
            (0, _) | (_, 127) if alpha != 127 => return self,
            (127, _) => return dst,
            _ => {}
        }
        let src_weight = 127 - alpha;
        let dst_weight = (127 - dst_alpha) * alpha / 127;
        let total = src_weight + dst_weight;
        let channel = |src, dst| ((src * src_weight + dst * dst_weight) / total) as u8;
        Self::gd(
            channel(red, dst_red),
            channel(green, dst_green),
            channel(blue, dst_blue),
            (alpha * dst_alpha / 127) as u8,
        )
    }
}

#[derive(Clone)]
pub enum Storage {
    TrueColor(Vec<Color>),
    Indexed {
        pixels: Vec<u8>,
        palette: Vec<Color>,
    },
}

#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub storage: Storage,
    pub blend: bool,
    pub save_alpha: bool,
    pub transparent: Option<i32>,
    pub interlaced: bool,
    pub resolution: [u32; 2],
    pub thickness: i32,
    pub antialias: bool,
    pub interpolation: i32,
}

pub fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let count = u64::from(width) * u64::from(height);
    if width > MAX_DIMENSION || height > MAX_DIMENSION || count == 0 || count > MAX_PIXELS as u64 {
        None
    } else {
        Some(count as usize)
    }
}

fn filled<T: Copy>(length: usize, value: T) -> Option<Vec<T>> {
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(length).ok()?;
    pixels.resize(length, value);
    Some(pixels)
}

impl Image {
    pub fn create(width: u32, height: u32, true_color: bool) -> Option<Self> {
        let length = pixel_count(width, height)?;
        let storage = if true_color {
            Storage::TrueColor(filled(length, Color::BLACK)?)
        } else {
            Storage::Indexed {
                pixels: filled(length, 0)?,
                palette: Vec::new(),
            }
        };
        Some(Self::with_storage(width, height, storage))
    }

    pub fn with_storage(width: u32, height: u32, storage: Storage) -> Self {
        Self {
            width,
            height,
            blend: matches!(storage, Storage::TrueColor(_)),
            storage,
            save_alpha: false,
            transparent: None,
            interlaced: false,
            resolution: [DEFAULT_DPI; 2],
            thickness: 1,
            antialias: false,
            interpolation: 3,
        }
    }

    pub fn is_true_color(&self) -> bool {
        matches!(self.storage, Storage::TrueColor(_))
    }

    pub fn location(&self, x: i64, y: i64) -> Option<usize> {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }

    pub fn index(&self, location: usize) -> i32 {
        match &self.storage {
            Storage::TrueColor(pixels) => pixels[location].0 as i32,
            Storage::Indexed { pixels, .. } => i32::from(pixels[location]),
        }
    }

    pub fn color(&self, index: i32) -> Option<Color> {
        let index = usize::try_from(index).ok()?;
        match &self.storage {
            Storage::TrueColor(_) => Some(Color(index as u32)),
            Storage::Indexed { palette, .. } => palette.get(index).copied(),
        }
    }

    pub fn true_pixel(&self, location: usize) -> Color {
        let index = self.index(location);
        self.color(index).unwrap_or(Color::BLACK)
    }

    pub fn set(&mut self, x: i64, y: i64, index: i32) {
        if let Some(location) = self.location(x, y) {
            self.set_at(location, index);
        }
    }

    pub fn set_at(&mut self, location: usize, index: i32) {
        if index < 0 {
            return;
        }
        match &mut self.storage {
            Storage::TrueColor(pixels) => {
                let color = Color(index as u32);
                let dst = &mut pixels[location];
                *dst = if self.blend { color.over(*dst) } else { color };
            }
            Storage::Indexed { pixels, .. } => pixels[location] = index as u8,
        }
    }

    pub fn allocate(&mut self, color: Color) -> Option<i32> {
        match &mut self.storage {
            Storage::TrueColor(_) => Some(color.0 as i32),
            Storage::Indexed { palette, .. } if palette.len() < 256 => {
                let index = palette.len() as i32;
                palette.push(color);
                Some(index)
            }
            Storage::Indexed { .. } => None,
        }
    }

    pub fn resolve(&mut self, color: Color) -> i32 {
        if let Storage::Indexed { palette, .. } = &self.storage
            && let Some(index) = palette.iter().position(|candidate| *candidate == color)
        {
            return index as i32;
        }
        self.allocate(color).unwrap_or_else(|| self.closest(color))
    }

    pub fn closest(&self, color: Color) -> i32 {
        let Storage::Indexed { palette, .. } = &self.storage else {
            return color.0 as i32;
        };
        let components = color.components();
        palette
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| {
                components
                    .into_iter()
                    .zip(candidate.components())
                    .map(|(a, b)| {
                        let delta = i32::from(a) - i32::from(b);
                        delta * delta
                    })
                    .sum::<i32>()
            })
            .map_or(-1, |(index, _)| index as i32)
    }

    pub fn palette_to_true_color(&mut self) -> bool {
        let Storage::Indexed { pixels, palette } = &self.storage else {
            return true;
        };
        let Some(mut colors) = filled(pixels.len(), Color::BLACK) else {
            return false;
        };
        for (output, index) in colors.iter_mut().zip(pixels) {
            let color = palette
                .get(usize::from(*index))
                .copied()
                .unwrap_or(Color::BLACK);
            *output = if Some(i32::from(*index)) == self.transparent {
                Color::TRANSPARENT
            } else {
                color
            };
        }
        self.transparent = self
            .transparent
            .and_then(|index| palette_color(palette, index).map(|color| color.0 as i32));
        self.storage = Storage::TrueColor(colors);
        self.blend = false;
        self.save_alpha = true;
        true
    }
}

fn palette_color(palette: &[Color], index: i32) -> Option<Color> {
    palette.get(usize::try_from(index).ok()?).copied()
}
