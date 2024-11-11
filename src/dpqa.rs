use std::fmt;

/// DPQA solver
pub struct DPQA {
    rows: u32,
    cols: u32,
    aod_rows: u32,
    aod_cols: u32,
}

impl DPQA {
    /// Create a new DPQA solver by specifying the size of the grid.
    pub fn new(rows: u32, cols: u32) -> DPQA {
        DPQA {
            rows: rows,
            cols: cols,
            aod_rows: rows,
            aod_cols: cols,
        }
    }

    /// Create a new DPQA solver by specifying the grid, potentially
    /// with a differently sized grid of AOD traps.
    pub fn new_aod(rows: u32, cols: u32, aod_rows: u32, aod_cols: u32) -> DPQA {
        DPQA {
            rows,
            cols,
            aod_rows,
            aod_cols,
        }
    }
}

impl fmt::Display for DPQA {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DPQA solver\n    grid:     {} x {}\n    AOD grid: {} x {}",
            self.rows, self.cols, self.aod_rows, self.aod_cols
        )
    }
}
