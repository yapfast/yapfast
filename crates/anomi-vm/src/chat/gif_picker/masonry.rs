//! Where the GIF picker's tiles go: in columns about 200 px wide, each tile
//! in the shortest column, leftmost on ties, as Discord's masonry places them
//! (`Masonry`, `src/ui/gifs.rs:60`-`:113`). The picture's height over its
//! width gives a GIF's height; the home's tiles and the placeholders are
//! fixed.

/// The space between columns and tiles, and above the first tile.
pub const GUTTER: f32 = 12.0;
/// The column width Discord aims for; a wider grid fits more columns.
pub const DESIRED_WIDTH: f32 = 200.0;
pub const MAX_COLUMNS: f32 = 8.0;

/// How tall a tile is in a column of a given width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A GIF's: its picture's height over its width.
    Picture(f32),
    /// The home's and placeholders': this tall, the picture cropped to the
    /// column's width.
    Fixed(f32),
}

impl Default for Shape {
    fn default() -> Self {
        Self::Picture(1.0)
    }
}

impl Shape {
    /// A GIF's picture, `width` by `height`; square when unknown.
    pub fn of(width: u32, height: u32) -> Self {
        if width == 0 || height == 0 {
            Self::Picture(1.0)
        } else {
            Self::Picture(height as f32 / width as f32)
        }
    }

    /// Its height in a column `width` wide.
    pub fn height(self, width: f32) -> f32 {
        match self {
            Self::Picture(ratio) => width * ratio,
            Self::Fixed(height) => height,
        }
    }

    /// The size to decode its picture at, in a column `width` wide: a home
    /// tile crops a picture to its width, and a placeholder has none.
    pub fn decoded(self, width: f32) -> (u32, u32) {
        match self {
            Self::Picture(_) => (width as u32, self.height(width) as u32),
            Self::Fixed(height) => (width as u32, height.max(width) as u32),
        }
    }
}

/// One tile's place, from its grid's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Slot {
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// The tiles placed: each one's slot, which tiles each column holds in
/// order, and how tall the grid is (its last tile's foot and a gutter).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placed {
    pub slots: Vec<Slot>,
    pub columns: Vec<Vec<usize>>,
    pub column_width: f32,
    pub height: f32,
}

/// How many columns a grid `width` wide holds, about 200 px each.
pub fn column_count(width: f32) -> usize {
    (width / DESIRED_WIDTH).floor().clamp(1.0, MAX_COLUMNS) as usize
}

/// How wide each of those columns is.
pub fn column_width(width: f32) -> f32 {
    let count = column_count(width) as f32;
    (width - GUTTER * (count - 1.0)) / count
}

/// Places tiles of these shapes in a grid `width` wide.
pub fn place(shapes: impl IntoIterator<Item = Shape>, width: f32) -> Placed {
    let count = column_count(width);
    let column_width = column_width(width);
    let mut feet = vec![GUTTER; count];
    let mut columns = vec![Vec::new(); count];
    let mut slots = Vec::new();
    for (index, shape) in shapes.into_iter().enumerate() {
        let height = shape.height(column_width);
        let (column, top) = feet
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(column, top)| (column, *top))
            .unwrap_or((0, GUTTER));
        feet[column] = top + height + GUTTER;
        columns[column].push(index);
        slots.push(Slot {
            x: GUTTER + column as f32 * (column_width + GUTTER),
            y: top,
            width: column_width,
            height,
        });
    }
    Placed {
        slots,
        columns,
        column_width,
        height: feet.into_iter().fold(0.0, f32::max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortest_column_takes_the_next_tile() {
        let placed = place(
            [
                Shape::Fixed(100.0),
                Shape::Fixed(300.0),
                Shape::Fixed(50.0),
                Shape::Fixed(50.0),
            ],
            474.0,
        );
        assert_eq!(placed.columns, vec![vec![0, 2, 3], vec![1]]);
        assert_eq!(placed.column_width, 231.0);
        assert_eq!((placed.slots[0].x, placed.slots[0].y), (12.0, 12.0));
        assert_eq!((placed.slots[1].x, placed.slots[1].y), (255.0, 12.0));
        assert_eq!(placed.slots[2].y, 124.0);
        assert_eq!(placed.slots[3].y, 186.0);
        assert_eq!(placed.height, 324.0);
        assert_eq!(Shape::of(220, 110).height(231.0), 115.5);
    }

    #[test]
    fn a_wider_grid_fits_more_columns_and_no_more_than_eight() {
        assert_eq!(column_count(474.0), 2);
        assert_eq!(column_count(624.0), 3);
        assert_eq!(column_width(624.0), 200.0);
        assert_eq!(column_count(10_000.0), 8);
        assert_eq!(column_count(50.0), 1);
    }

    #[test]
    fn nothing_placed_is_a_gutter_tall() {
        assert_eq!(place([], 474.0).height, GUTTER);
    }
}
