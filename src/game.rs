pub use crate::{Player, Square};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use shakmaty::{
    fen::Fen, CastlingMode, Chess, Color as ShakmColor, File as ShakmFile,
    Move as ShakmMove, MoveList, Piece as ShakmPiece, Position, Rank as ShakmRank,
    Role, Square as ShakmSquare,
};

// Re-export shakmaty types used by other modules
pub use shakmaty::{
    File as ChessFile, Piece as ChessPiece, Rank as ChessRank, Role as ChessRole,
    Square as ChessSquare,
};

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChessOutcome {
    Checkmate { winner: Player },
    Stalemate,
    Aborted { who: Option<Player> },
}

/// Wrapper around shakmaty's Chess position.
/// Maintains a history stack for undo support.
pub struct ChessGame {
    position: Chess,
    history: Vec<Chess>,
    board_moves_played_offset: u16,
    moves_played: u16,
    outcome: Option<ChessOutcome>,
}

impl Default for ChessGame {
    fn default() -> Self {
        Self {
            position: Chess::default(),
            history: Vec::new(),
            board_moves_played_offset: 0,
            moves_played: 0,
            outcome: None,
        }
    }
}

impl ChessGame {
    pub fn from_fen(fen: &str) -> Result<ChessGame> {
        let parsed: Fen = fen.parse().map_err(|e| anyhow!(
            "Failed to parse FEN string: {:?}", e
        ))?;
        let position: Chess = parsed.into_position(CastlingMode::Standard).map_err(|e| anyhow!(
            "Failed to create game board from FEN. Reason: {:?}", e
        ))?;
        // Count half-moves from FEN fullmove number
        let initial_moves = 0u16; // shakmaty doesn't track total moves; we track manually
        Ok(Self {
            position,
            history: Vec::new(),
            board_moves_played_offset: initial_moves,
            moves_played: initial_moves,
            outcome: None,
        })
    }

    pub fn position(&self) -> &Chess {
        &self.position
    }

    pub fn fen(&self) -> String {
        let fen = Fen::from_position(&self.position, shakmaty::EnPassantMode::Legal);
        fen.to_string()
    }

    pub fn turn(&self) -> Player {
        self.position.turn().into()
    }

    pub fn outcome(&self) -> Option<ChessOutcome> {
        self.outcome
    }

    pub fn total_moves(&self) -> u16 {
        self.moves_played
    }

    pub fn total_undoable_moves(&self) -> u16 {
        self.history.len() as u16
    }

    pub fn possible_moves(&self) -> MoveList {
        self.position.legal_moves()
    }

    pub fn player_left(&mut self, player: Player) {
        if self.outcome.is_none() {
            self.outcome = Some(ChessOutcome::Aborted { who: Some(player) });
        }
    }

    pub fn undo(&mut self, count: u16) -> Result<()> {
        if count as usize > self.history.len() {
            return Err(anyhow!(
                "Can't undo {} moves. Only {} moves available to undo.",
                count, self.history.len()
            ));
        }

        for _ in 0..count {
            if let Some(prev) = self.history.pop() {
                self.position = prev;
                self.moves_played -= 1;
            }
        }
        self.update_game_outcome();
        Ok(())
    }

    fn piece_on_square(&self, player: Player, square: Square) -> bool {
        let color: ShakmColor = player.into();
        let board = self.position.board();
        let occupied = board.by_color(color);
        occupied.contains(square.inner())
    }

    fn update_game_outcome(&mut self) {
        if self.position.is_checkmate() {
            self.outcome = Some(ChessOutcome::Checkmate {
                winner: self.turn().other_player(),
            });
        } else if self.position.is_stalemate() {
            self.outcome = Some(ChessOutcome::Stalemate);
        } else if let Some(outcome) = self.outcome {
            match outcome {
                ChessOutcome::Aborted { .. } => {} // Abort is irreversible
                _ => self.outcome = None,
            };
        }
    }

    pub fn move_piece_by_type(
        &mut self,
        piece: ShakmPiece,
        destination: Square,
        src_col: Option<ShakmFile>,
        src_row: Option<ShakmRank>,
    ) -> Result<(Square, Square)> {
        ensure!(
            self.outcome.is_none(),
            "Can't do move since the game has already ended."
        );

        let legal_moves = self.position.legal_moves();
        let mut candidate_moves: Vec<&ShakmMove> = Vec::new();

        for legal_move in legal_moves.iter() {
            let (src, dst, role) = match legal_move {
                ShakmMove::Normal { from, to, role, .. } => (*from, *to, *role),
                ShakmMove::Castle { king, rook } => {
                    // For castling, the king's destination depends on side
                    let king_dst = if rook.file() > king.file() {
                        // Kingside
                        ShakmSquare::from_coords(ShakmFile::G, king.rank())
                    } else {
                        // Queenside
                        ShakmSquare::from_coords(ShakmFile::C, king.rank())
                    };
                    (*king, king_dst, Role::King)
                }
                ShakmMove::EnPassant { from, to } => (*from, *to, Role::Pawn),
                ShakmMove::Put { .. } => continue,
            };

            // Check this move involves the right piece type at a source square
            let move_piece = ShakmPiece { color: self.position.turn(), role };
            if move_piece == piece && dst == destination.inner() {
                candidate_moves.push(legal_move);
            }
        }

        if candidate_moves.is_empty() {
            return Err(anyhow!("Move not found as possibility"));
        }

        let mut selected_move: Option<&ShakmMove> = None;
        if candidate_moves.len() == 1 {
            selected_move = Some(candidate_moves[0]);
        }

        if selected_move.is_none() && src_col.is_some() {
            selected_move = candidate_moves.iter().find(|m| {
                move_src(m).map(|s| s.file()) == src_col
            }).copied();
        }
        if selected_move.is_none() && src_row.is_some() {
            selected_move = candidate_moves.iter().find(|m| {
                move_src(m).map(|s| s.rank()) == src_row
            }).copied();
        }

        if selected_move.is_none() {
            return Err(anyhow!("Move not found as possibility"));
        }

        let selected_move = selected_move.unwrap().clone();
        let src_sq = move_src(&selected_move).unwrap_or(destination.inner());

        self.history.push(self.position.clone());
        self.position.play_unchecked(selected_move);
        self.moves_played += 1;

        self.update_game_outcome();
        Ok((Square::from(src_sq), destination))
    }

    pub fn move_piece(&mut self, source: Square, destination: Square) -> Result<()> {
        ensure!(
            self.piece_on_square(self.turn(), source),
            "The playing player has no piece on the source square!"
        );
        ensure!(source != destination, "Move does not actually move");
        ensure!(
            self.outcome.is_none(),
            "Can't do move since the game has already ended."
        );

        let legal_moves = self.position.legal_moves();
        let mut selected_move: Option<ShakmMove> = None;

        for legal_move in legal_moves.iter() {
            let (src, dst) = match legal_move {
                ShakmMove::Normal { from, to, .. } => (*from, *to),
                ShakmMove::Castle { king, rook } => {
                    // User clicks king square → rook square for castling
                    (*king, *rook)
                }
                ShakmMove::EnPassant { from, to } => (*from, *to),
                ShakmMove::Put { .. } => continue,
            };
            if src == source.inner() && dst == destination.inner() {
                selected_move = Some(legal_move.clone());
            }
        }

        // Also try castling by king destination squares (G1/C1/G8/C8)
        if selected_move.is_none() {
            for legal_move in legal_moves.iter() {
                if let ShakmMove::Castle { king, rook } = legal_move {
                    let king_dst = if rook.file() > king.file() {
                        ShakmSquare::from_coords(ShakmFile::G, king.rank())
                    } else {
                        ShakmSquare::from_coords(ShakmFile::C, king.rank())
                    };
                    if *king == source.inner() && king_dst == destination.inner() {
                        selected_move = Some(legal_move.clone());
                    }
                }
            }
        }

        if selected_move.is_none() {
            return Err(anyhow!("Move not found as possibility"));
        }

        let selected_move = selected_move.unwrap();
        self.history.push(self.position.clone());
        self.position.play_unchecked(selected_move);
        self.moves_played += 1;

        self.update_game_outcome();
        Ok(())
    }
}

/// Extract the source square from a shakmaty Move.
pub fn move_src(m: &ShakmMove) -> Option<ShakmSquare> {
    match m {
        ShakmMove::Normal { from, .. } => Some(*from),
        ShakmMove::Castle { king, .. } => Some(*king),
        ShakmMove::EnPassant { from, .. } => Some(*from),
        ShakmMove::Put { .. } => None,
    }
}

/// Extract the destination square from a shakmaty Move.
pub fn move_dst(m: &ShakmMove) -> ShakmSquare {
    match m {
        ShakmMove::Normal { to, .. } => *to,
        ShakmMove::Castle { king, rook } => {
            if rook.file() > king.file() {
                ShakmSquare::from_coords(ShakmFile::G, king.rank())
            } else {
                ShakmSquare::from_coords(ShakmFile::C, king.rank())
            }
        }
        ShakmMove::EnPassant { to, .. } => *to,
        ShakmMove::Put { to, .. } => *to,
    }
}

/// Map a shakmaty Piece to a character for display purposes.
pub fn piece_to_char(piece: ShakmPiece) -> char {
    let c = match piece.role {
        Role::Pawn => 'P',
        Role::Knight => 'N',
        Role::Bishop => 'B',
        Role::Rook => 'R',
        Role::Queen => 'Q',
        Role::King => 'K',
    };
    if piece.color == ShakmColor::Black {
        c.to_ascii_lowercase()
    } else {
        c
    }
}
