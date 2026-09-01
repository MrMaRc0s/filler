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
}
