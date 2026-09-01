//! Core game logic for the filler bot, kept separate from main.rs so it
//! can be exercised directly by unit tests without going through stdin.

/// Which side of the match we are playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    One,
    Two,
}

impl Player {
    pub fn own_symbols(self) -> [char; 2] {
        match self {
            Player::One => ['a', '@'],
            Player::Two => ['s', '$'],
        }
    }

    pub fn opp_symbols(self) -> [char; 2] {
        match self {
            Player::One => ['s', '$'],
            Player::Two => ['a', '@'],
        }
    }

    pub fn is_own(self, c: char) -> bool {
        self.own_symbols().contains(&c)
    }

    pub fn is_opp(self, c: char) -> bool {
        self.opp_symbols().contains(&c)
    }
}

/// The Anfield: a rectangular grid of territory/empty cells.
#[derive(Debug, Clone)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    cells: Vec<char>,
}

impl Grid {
    pub fn new(width: usize, height: usize, cells: Vec<char>) -> Self {
        assert_eq!(cells.len(), width * height, "grid cell count must match width*height");
        Grid { width, height, cells }
    }

    /// Builds a grid from raw row strings (as sent by the game_engine, with
    /// the leading "NNN " row-index prefix already stripped). Rows shorter
    /// than `width` are padded with '.', matching an empty Anfield cell.
    pub fn from_rows(width: usize, height: usize, rows: &[String]) -> Self {
        let mut cells = Vec::with_capacity(width * height);
        for row in rows {
            let chars: Vec<char> = row.chars().collect();
            for x in 0..width {
                cells.push(*chars.get(x).unwrap_or(&'.'));
            }
        }
        Grid::new(width, height, cells)
    }

    pub fn get(&self, x: usize, y: usize) -> char {
        self.cells[y * self.width + x]
    }

    pub fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }
}

/// A game piece, as sent by the game_engine: a bounding box that may have
/// empty padding on any side, with only the non-'.' cells actually occupied.
#[derive(Debug, Clone)]
pub struct Piece {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<(usize, usize)>,
}

impl Piece {
    pub fn from_rows(width: usize, height: usize, rows: &[String]) -> Self {
        let mut cells = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            let chars: Vec<char> = row.chars().collect();
            for x in 0..width {
                if let Some(&c) = chars.get(x) {
                    if c != '.' {
                        cells.push((x, y));
                    }
                }
            }
        }
        Piece { width, height, cells }
    }

    pub fn area(&self) -> usize {
        self.cells.len()
    }
}

/// Parses a "<Keyword> <w> <h>:" header line, e.g. "Anfield 20 15:" or
/// "Piece 4 3:". Returns None if the line doesn't start with `keyword`
/// or the dimensions can't be parsed.
pub fn parse_dims(line: &str, keyword: &str) -> Option<(usize, usize)> {
    let line = line.trim();
    if !line.starts_with(keyword) {
        return None;
    }
    let rest = line[keyword.len()..].trim_end_matches(':').trim();
    let mut parts = rest.split_whitespace();
    let w = parts.next()?.parse().ok()?;
    let h = parts.next()?.parse().ok()?;
    Some((w, h))
}

/// Parses the opening "$$$ exec p<N> : [<path>]" line to find which player
/// we are. Reads the player token positionally rather than substring
/// searching, so a robot path that happens to contain "p1"/"p2" can't
/// confuse it.
pub fn parse_player(line: &str) -> Option<Player> {
    match line.split_whitespace().nth(2)? {
        "p1" => Some(Player::One),
        "p2" => Some(Player::Two),
        _ => None,
    }
}

/// Strips the "NNN " row-index prefix the engine puts in front of each
/// Anfield row, returning just the cell content.
pub fn row_content(line: &str) -> &str {
    match line.find(' ') {
        Some(idx) => &line[idx + 1..],
        None => line,
    }
}

/// A piece placement that has already been checked against the board:
/// its bounding-box origin plus the absolute cells it would occupy.
#[derive(Debug, Clone)]
pub struct Placement {
    pub x: usize,
    pub y: usize,
    pub cells: Vec<(usize, usize)>,
}

/// Checks whether `piece` can legally be placed with its bounding-box
/// origin at (ox, oy): every occupied cell must land inside the grid,
/// exactly one must overlap our own territory, and none may overlap the
/// opponent's.
pub fn evaluate_placement(
    grid: &Grid,
    piece: &Piece,
    ox: usize,
    oy: usize,
    player: Player,
) -> Option<Placement> {
    let mut overlap_mine = 0u32;
    let mut overlap_opp = 0u32;
    let mut cells = Vec::with_capacity(piece.cells.len());
    for &(px, py) in &piece.cells {
        let gx = ox as i64 + px as i64;
        let gy = oy as i64 + py as i64;
        if !grid.in_bounds(gx, gy) {
            return None;
        }
        let (gx, gy) = (gx as usize, gy as usize);
        let c = grid.get(gx, gy);
        if player.is_own(c) {
            overlap_mine += 1;
        } else if player.is_opp(c) {
            overlap_opp += 1;
        }
        cells.push((gx, gy));
    }
    if overlap_mine == 1 && overlap_opp == 0 {
        Some(Placement { x: ox, y: oy, cells })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_dims_reads_anfield_header() {
        assert_eq!(parse_dims("Anfield 20 15:", "Anfield"), Some((20, 15)));
    }

    #[test]
    fn parse_dims_reads_piece_header() {
        assert_eq!(parse_dims("Piece 4 3:", "Piece"), Some((4, 3)));
    }

    #[test]
    fn parse_dims_rejects_wrong_keyword() {
        assert_eq!(parse_dims("Piece 4 3:", "Anfield"), None);
    }

    #[test]
    fn parse_player_reads_p1_and_p2() {
        assert_eq!(parse_player("$$$ exec p1 : [robots/bender]"), Some(Player::One));
        assert_eq!(parse_player("$$$ exec p2 : [robots/wall_e]"), Some(Player::Two));
    }

    #[test]
    fn parse_player_ignores_p1_p2_substrings_in_path() {
        // A path containing "p1" shouldn't be mistaken for the player token,
        // since the actual token is picked positionally, not by substring.
        assert_eq!(
            parse_player("$$$ exec p2 : [/home/team/p1_backup/robot]"),
            Some(Player::Two)
        );
    }

    #[test]
    fn row_content_strips_row_index_prefix() {
        assert_eq!(row_content("012 .........@.........."), ".........@..........");
    }

    #[test]
    fn grid_from_rows_builds_expected_cells() {
        let grid = Grid::from_rows(4, 2, &rows(&["a.@.", "..s$"]));
        assert_eq!(grid.get(0, 0), 'a');
        assert_eq!(grid.get(1, 0), '.');
        assert_eq!(grid.get(2, 0), '@');
        assert_eq!(grid.get(2, 1), 's');
        assert_eq!(grid.get(3, 1), '$');
    }

    #[test]
    fn piece_from_rows_collects_only_occupied_cells() {
        // "Piece 4 1: .OO." from the subject's own example.
        let piece = Piece::from_rows(4, 1, &rows(&[".OO."]));
        assert_eq!(piece.cells, vec![(1, 0), (2, 0)]);
        assert_eq!(piece.area(), 2);
    }

    #[test]
    fn piece_from_rows_handles_padding_on_any_side() {
        // Real engine output: a piece whose bounding box has empty rows
        // below the only occupied row.
        let piece = Piece::from_rows(4, 6, &rows(&["OO..", "....", "....", "....", "....", "...."]));
        assert_eq!(piece.cells, vec![(0, 0), (1, 0)]);
    }

    // --- Placement validation -----------------------------------------

    #[test]
    fn placement_accepted_with_exactly_one_overlap() {
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", "..@..", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        // Origin (1,2): cells (1,2) empty, (2,2) is '@' -> one overlap.
        let placement = evaluate_placement(&grid, &piece, 1, 2, Player::One);
        assert!(placement.is_some());
        assert_eq!(placement.unwrap().cells, vec![(1, 2), (2, 2)]);
    }

    #[test]
    fn placement_rejected_with_zero_overlap() {
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", "..@..", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        // Origin (0,0): neither cell touches the existing '@'.
        assert!(evaluate_placement(&grid, &piece, 0, 0, Player::One).is_none());
    }

    #[test]
    fn placement_rejected_with_two_overlaps_on_own_territory() {
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", ".@@..", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        // Both piece cells land on our own territory.
        assert!(evaluate_placement(&grid, &piece, 1, 2, Player::One).is_none());
    }

    #[test]
    fn placement_rejected_when_touching_opponent_territory() {
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", ".@s..", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        // One cell overlaps our own '@', the other overlaps opponent 's'.
        assert!(evaluate_placement(&grid, &piece, 1, 2, Player::One).is_none());
    }

    #[test]
    fn placement_treats_last_placed_and_settled_symbols_as_same_owner() {
        // Lowercase 'a' (last placed) and '@' (settled) are both player 1.
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", "..a..", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        assert!(evaluate_placement(&grid, &piece, 1, 2, Player::One).is_some());
    }

    // --- Boundary conditions -------------------------------------------

    #[test]
    fn placement_rejected_past_right_edge() {
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", "....@", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        // Origin x=4 pushes the second cell to x=5, outside a width-5 grid.
        assert!(evaluate_placement(&grid, &piece, 4, 2, Player::One).is_none());
    }

    #[test]
    fn placement_rejected_past_bottom_edge() {
        let grid = Grid::from_rows(5, 5, &rows(&[".....", ".....", ".....", ".....", "....@"]));
        let piece = Piece::from_rows(1, 2, &rows(&["#", "#"]));
        // Origin y=4 pushes the second cell to y=5, outside a height-5 grid.
        assert!(evaluate_placement(&grid, &piece, 4, 4, Player::One).is_none());
    }

    #[test]
    fn placement_accepted_when_anchored_at_the_very_edge() {
        let grid = Grid::from_rows(5, 5, &rows(&["@....", ".....", ".....", ".....", "....."]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        // ox=0 is the smallest legal origin; confirms the search space
        // starts at the edge itself rather than needing to go negative.
        assert!(evaluate_placement(&grid, &piece, 0, 0, Player::One).is_some());
    }
}
