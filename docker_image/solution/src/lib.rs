//! Core game logic for the filler bot, kept separate from main.rs so it
//! can be exercised directly by unit tests without going through stdin.

use std::collections::{HashSet, VecDeque};

const NEIGHBORS: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

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

/// Formats a chosen coordinate as the exact "X Y\n" line the game_engine
/// expects on stdout.
pub fn format_answer(x: usize, y: usize) -> String {
    format!("{} {}\n", x, y)
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

/// BFS distance, from every cell, to the nearest source cell.
/// Unreached cells (shouldn't happen on the open Anfield) stay at u32::MAX.
fn bfs_multi_source(grid: &Grid, sources: &[(usize, usize)]) -> Vec<u32> {
    let mut dist = vec![u32::MAX; grid.width * grid.height];
    let mut q = VecDeque::new();
    for &(x, y) in sources {
        let idx = y * grid.width + x;
        if dist[idx] == u32::MAX {
            dist[idx] = 0;
            q.push_back((x, y));
        }
    }
    while let Some((x, y)) = q.pop_front() {
        let d = dist[y * grid.width + x];
        for (dx, dy) in NEIGHBORS {
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
            if grid.in_bounds(nx, ny) {
                let (nx, ny) = (nx as usize, ny as usize);
                let idx = ny * grid.width + nx;
                if dist[idx] == u32::MAX {
                    dist[idx] = d + 1;
                    q.push_back((nx, ny));
                }
            }
        }
    }
    dist
}

/// Counts how many empty cells are reachable from `start_cells` through
/// other empty cells, stopping early once `cap` is reached. Used as a
/// cheap "how much room is actually here" safety check so we don't wander
/// into a pocket too small to keep growing in.
fn flood_size(grid: &Grid, start_cells: &[(usize, usize)], cap: usize) -> usize {
    let mut seen: HashSet<(usize, usize)> = start_cells.iter().copied().collect();
    let mut q: VecDeque<(usize, usize)> = start_cells.iter().copied().collect();
    let mut count = 0;
    while let Some((x, y)) = q.pop_front() {
        if count >= cap {
            break;
        }
        for (dx, dy) in NEIGHBORS {
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
            if grid.in_bounds(nx, ny) {
                let (nx, ny) = (nx as usize, ny as usize);
                if grid.get(nx, ny) == '.' && seen.insert((nx, ny)) {
                    count += 1;
                    if count >= cap {
                        break;
                    }
                    q.push_back((nx, ny));
                }
            }
        }
    }
    count
}

// Below this size the whole Anfield is small enough that growing outward
// (away from existing territory) tends to fragment us into corners; hugging
// our own territory instead keeps more options open. Tuned against the
// reference robots on the 20x15 map.
const SMALL_BOARD_AREA: usize = 600;
// Only run the (more expensive) flood-fill safety check on the most
// promising candidates instead of every legal placement.
const TOP_CANDIDATES: usize = 30;
// A candidate needs at least this many times its own area of reachable
// open space nearby to be considered "safe" rather than a potential
// dead end.
const SAFETY_MULTIPLIER: usize = 6;

/// Picks the best legal placement for `piece` on `grid`, or None if no
/// legal placement exists (caller should then answer "0 0").
///
/// Candidates are ranked by distance from existing territory (favoring
/// wide-open growth on large boards, compact growth on small ones) plus a
/// bonus for crowding the opponent, then filtered for a minimum amount of
/// reachable open space so we don't grow into a dead end.
pub fn best_move(grid: &Grid, piece: &Piece, player: Player) -> Option<(usize, usize)> {
    if piece.cells.is_empty() {
        return None;
    }

    let obstacle_sources: Vec<(usize, usize)> = (0..grid.height)
        .flat_map(|y| (0..grid.width).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.get(x, y) != '.')
        .collect();
    let dist_wall = bfs_multi_source(grid, &obstacle_sources);
    let small_board = grid.width * grid.height < SMALL_BOARD_AREA;

    let mut candidates: Vec<(i64, Placement)> = Vec::new();
    for oy in 0..grid.height {
        for ox in 0..grid.width {
            let placement = match evaluate_placement(grid, piece, ox, oy, player) {
                Some(p) => p,
                None => continue,
            };

            let wall_sum: i64 = placement
                .cells
                .iter()
                .map(|&(gx, gy)| dist_wall[gy * grid.width + gx] as i64)
                .sum();
            let wall_term = if small_board { -wall_sum } else { wall_sum };

            let mut opp_adj = 0i64;
            for &(gx, gy) in &placement.cells {
                for (dx, dy) in NEIGHBORS {
                    let nx = gx as i64 + dx;
                    let ny = gy as i64 + dy;
                    if grid.in_bounds(nx, ny) && player.is_opp(grid.get(nx as usize, ny as usize)) {
                        opp_adj += 1;
                    }
                }
            }

            let score = wall_term + opp_adj * 6;
            candidates.push((score, placement));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.truncate(TOP_CANDIDATES);

    let safe_threshold = piece.area() * SAFETY_MULTIPLIER;
    let fallback = (candidates[0].1.x, candidates[0].1.y);
    for (_, placement) in &candidates {
        if flood_size(grid, &placement.cells, safe_threshold + 1) >= safe_threshold {
            return Some((placement.x, placement.y));
        }
    }
    Some(fallback)
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

    // --- Move selection --------------------------------------------------

    #[test]
    fn best_move_never_returns_out_of_bounds_coordinates() {
        let grid = Grid::from_rows(6, 6, &rows(&[
            "......", "......", "......", "...@..", "......", "......",
        ]));
        let piece = Piece::from_rows(3, 3, &rows(&["###", "###", "###"]));
        let (x, y) = best_move(&grid, &piece, Player::One).expect("a legal placement exists");
        for &(px, py) in &piece.cells {
            assert!(grid.in_bounds((x + px) as i64, (y + py) as i64));
        }
    }

    #[test]
    fn best_move_returns_none_when_no_legal_placement_exists() {
        // A 1-cell piece could trivially "overlap" any of our own already
        // occupied cells, so use a 2-cell piece: with the board fully
        // occupied there's nowhere for its second cell to land.
        let grid = Grid::from_rows(2, 2, &rows(&["@@", "@@"]));
        let piece = Piece::from_rows(2, 1, &rows(&["##"]));
        assert_eq!(best_move(&grid, &piece, Player::One), None);
    }

    // --- Coordinate output -------------------------------------------

    #[test]
    fn format_answer_matches_required_wire_format() {
        assert_eq!(format_answer(7, 2), "7 2\n");
        assert_eq!(format_answer(0, 0), "0 0\n");
    }
}
