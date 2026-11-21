use std::io::{self, stdout, Stdout, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};

const W: usize = 10;
const H: usize = 20;

// 7 piece shapes, each made of 4 cells (x, y)
const SHAPES: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)], // I
    [(0, 0), (1, 0), (0, 1), (1, 1)], // O
    [(1, 0), (0, 1), (1, 1), (2, 1)], // T
    [(1, 0), (2, 0), (0, 1), (1, 1)], // S
    [(0, 0), (1, 0), (1, 1), (2, 1)], // Z
    [(0, 0), (0, 1), (1, 1), (2, 1)], // J
    [(2, 0), (0, 1), (1, 1), (2, 1)], // L
];
const COLORS: [Color; 7] = [
    Color::Cyan,
    Color::Yellow,
    Color::Magenta,
    Color::Green,
    Color::Red,
    Color::Blue,
    Color::DarkYellow,
];

#[derive(Clone, Copy)]
struct Piece {
    kind: usize,
    cells: [(i32, i32); 4],
    x: i32,
    y: i32,
}

struct Game {
    board: [[Option<Color>; W]; H],
    cur: Piece,
    score: u32,
    over: bool,
    rng: u64,
}

impl Game {
    fn new() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        let mut g = Game {
            board: [[None; W]; H],
            cur: Piece { kind: 0, cells: SHAPES[0], x: 0, y: 0 },
            score: 0,
            over: false,
            rng: seed | 1,
        };
        g.cur = g.spawn();
        g
    }

    // Simple xorshift RNG, so we do not need the `rand` crate
    fn rand(&mut self) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 7) as usize
    }

    fn spawn(&mut self) -> Piece {
        let kind = self.rand();
        Piece { kind, cells: SHAPES[kind], x: 3, y: 0 }
    }

    // Does the piece fit here (inside the board and not overlapping)?
    fn fits(&self, p: &Piece) -> bool {
        p.cells.iter().all(|&(cx, cy)| {
            let (x, y) = (p.x + cx, p.y + cy);
            x >= 0
                && x < W as i32
                && y < H as i32
                && (y < 0 || self.board[y as usize][x as usize].is_none())
        })
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let mut p = self.cur;
        p.x += dx;
        p.y += dy;
        if self.fits(&p) {
            self.cur = p;
            true
        } else {
            false
        }
    }

    fn rotate(&mut self) {
        if self.cur.kind == 1 {
            return; // O piece does not need rotating
        }
        let n = if self.cur.kind == 0 { 4 } else { 3 };
        let mut p = self.cur;
        for c in p.cells.iter_mut() {
            *c = (n - 1 - c.1, c.0); // rotate 90° clockwise
        }
        // Simple "wall kick": try shifting if blocked by a wall
        for dx in [0, -1, 1, -2, 2] {
            let mut k = p;
            k.x += dx;
            if self.fits(&k) {
                self.cur = k;
                return;
            }
        }
    }

    fn lock(&mut self) {
        let color = COLORS[self.cur.kind];
        for &(cx, cy) in &self.cur.cells {
            let (x, y) = (self.cur.x + cx, self.cur.y + cy);
            if y < 0 {
                self.over = true;
                return;
            }
            self.board[y as usize][x as usize] = Some(color);
        }
        self.clear_lines();
        self.cur = self.spawn();
        if !self.fits(&self.cur) {
            self.over = true;
        }
    }

    fn clear_lines(&mut self) {
        let mut new = [[None; W]; H];
        let mut row = H;
        let mut cleared = 0;
        for y in (0..H).rev() {
            if self.board[y].iter().all(|c| c.is_some()) {
                cleared += 1;
            } else {
                row -= 1;
                new[row] = self.board[y];
            }
        }
        self.board = new;
        self.score += [0, 100, 300, 500, 800][cleared];
    }

    fn tick(&mut self) {
        if !self.try_move(0, 1) {
            self.lock();
        }
    }

    fn hard_drop(&mut self) {
        while self.try_move(0, 1) {}
        self.lock();
    }

    fn draw(&self, out: &mut Stdout) -> io::Result<()> {
        queue!(out, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All))?;
        for y in 0..H {
            queue!(out, cursor::MoveTo(0, y as u16), Print("│"))?;
            for x in 0..W {
                let active = self.cur.cells.iter().any(|&(cx, cy)| {
                    self.cur.x + cx == x as i32 && self.cur.y + cy == y as i32
                });
                let cell = if active { Some(COLORS[self.cur.kind]) } else { self.board[y][x] };
                match cell {
                    Some(c) => queue!(out, SetForegroundColor(c), Print("██"), ResetColor)?,
                    None => queue!(out, Print(" ·"))?,
                }
            }
            queue!(out, Print("│"))?;
        }
        queue!(
            out,
            cursor::MoveTo(0, H as u16),
            Print(format!("└{}┘", "──".repeat(W))),
            cursor::MoveTo(W as u16 * 2 + 4, 1),
            Print(format!("Score: {}", self.score)),
            cursor::MoveTo(W as u16 * 2 + 4, 3),
            Print("← →  : move"),
            cursor::MoveTo(W as u16 * 2 + 4, 4),
            Print("↑    : rotate"),
            cursor::MoveTo(W as u16 * 2 + 4, 5),
            Print("↓    : soft drop"),
            cursor::MoveTo(W as u16 * 2 + 4, 6),
            Print("space: hard drop"),
            cursor::MoveTo(W as u16 * 2 + 4, 7),
            Print("q    : quit"),
        )?;
        out.flush()
    }
}

// Draw a "game over" box on top of the board.
fn draw_game_over(out: &mut Stdout, score: u32) -> io::Result<()> {
    let lines = [
        String::new(),
        "GAME OVER".to_string(),
        format!("Score: {score}"),
        String::new(),
        "r : restart".to_string(),
        "q : quit".to_string(),
        String::new(),
    ];
    let top = (H - lines.len()) as u16 / 2;
    for (i, text) in lines.iter().enumerate() {
        queue!(out, cursor::MoveTo(1, top + i as u16))?;
        if i == 1 {
            queue!(out, SetForegroundColor(Color::Red))?;
        }
        queue!(out, Print(format!("{:^width$}", text, width = W * 2)), ResetColor)?;
    }
    out.flush()
}

// Keep playing rounds until the player quits. Returns the last score.
fn run(out: &mut Stdout) -> io::Result<u32> {
    loop {
        let (score, quit) = play(out)?;
        if quit {
            return Ok(score);
        }

        draw_game_over(out, score)?;
        loop {
            if let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
            {
                match k.code {
                    KeyCode::Char('r') | KeyCode::Enter => break, // start a new round
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(score),
                    _ => {}
                }
            }
        }
    }
}

// Play one round. Returns (score, true if the player quit instead of losing).
fn play(out: &mut Stdout) -> io::Result<(u32, bool)> {
    let mut g = Game::new();
    let mut last = Instant::now();
    let speed = Duration::from_millis(500);

    while !g.over {
        g.draw(out)?;
        let timeout = speed.saturating_sub(last.elapsed());
        if event::poll(timeout)?
            && let Event::Key(k) = event::read()?
            && k.kind == KeyEventKind::Press
        {
            match k.code {
                KeyCode::Left => { g.try_move(-1, 0); }
                KeyCode::Right => { g.try_move(1, 0); }
                KeyCode::Down => { g.try_move(0, 1); }
                KeyCode::Up => g.rotate(),
                KeyCode::Char(' ') => g.hard_drop(),
                KeyCode::Char('q') | KeyCode::Esc => return Ok((g.score, true)),
                _ => {}
            }
        }
        if last.elapsed() >= speed {
            g.tick();
            last = Instant::now();
        }
    }
    g.draw(out)?; // show the final board under the game-over box
    Ok((g.score, false))
}

fn main() -> io::Result<()> {
    let mut out = stdout();
    terminal::enable_raw_mode()?;
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;

    let result = run(&mut out); // keep the result so the terminal is always restored

    execute!(out, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    println!("Thanks for playing! Final score: {}", result?);
    Ok(())
}
