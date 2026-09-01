use std::io::{self, BufRead, Write};

use filler::{best_move, format_answer, parse_dims, parse_player, row_content, Grid, Piece};

/// Reads one line from the game_engine, stripping the trailing newline.
/// Returns None on EOF (the engine has closed our stdin, i.e. the game is
/// over).
fn read_line(reader: &mut impl BufRead) -> Option<String> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).ok()?;
    if n == 0 {
        return None;
    }
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    Some(line)
}

/// Reads one full turn (an Anfield block followed by a Piece block) off
/// the same long-lived stdin stream. Returns None once the engine has
/// nothing left to send us.
fn read_turn(reader: &mut impl BufRead) -> Option<(Grid, Piece)> {
    let mut anfield_line = read_line(reader)?;
    while anfield_line.trim().is_empty() {
        anfield_line = read_line(reader)?;
    }
    let (w, h) = parse_dims(&anfield_line, "Anfield")?;

    let _column_header = read_line(reader)?;
    let mut rows = Vec::with_capacity(h);
    for _ in 0..h {
        let line = read_line(reader)?;
        rows.push(row_content(&line).to_string());
    }
    let grid = Grid::from_rows(w, h, &rows);

    let piece_line = read_line(reader)?;
    let (pw, ph) = parse_dims(&piece_line, "Piece")?;
    let mut piece_rows = Vec::with_capacity(ph);
    for _ in 0..ph {
        piece_rows.push(read_line(reader)?);
    }
    let piece = Piece::from_rows(pw, ph, &piece_rows);

    Some((grid, piece))
}

fn main() {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    let first_line = match read_line(&mut reader) {
        Some(line) => line,
        None => return,
    };
    let player = match parse_player(&first_line) {
        Some(p) => p,
        None => return,
    };

    while let Some((grid, piece)) = read_turn(&mut reader) {
        let (x, y) = best_move(&grid, &piece, player).unwrap_or((0, 0));
        if writer.write_all(format_answer(x, y).as_bytes()).is_err() {
            break;
        }
        if writer.flush().is_err() {
            break;
        }
    }
}
