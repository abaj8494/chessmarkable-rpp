use crate::game::ChessGame;
pub use crate::game::ChessOutcome;
use crate::Square;
use chess_pgn_parser::File as LocalFile;
use chess_pgn_parser::Piece as LocalPiece;
use chess_pgn_parser::Rank as LocalRank;
use chess_pgn_parser::{Game, GameMove, GameTermination, Move};
use shakmaty::{
    Color as ShakmColor, File as ShakmFile, Piece as ShakmPiece, Position,
    Rank as ShakmRank, Role, Square as ShakmSquare,
};

const FEN_TAG: &str = "FEN";

pub struct ReplayResponse {
    pub fen: String,
    pub comment: Option<String>,
    pub last_move_from: Option<Square>,
    pub last_move_to: Option<Square>,
}

pub struct Replay {
    active_game: ChessGame,
    replay_info: Game,
    replay_moves_played_offset: usize,
    player_moves_played_offset: usize,
    is_white_turn: bool,
}

impl Replay {
    pub fn new(replay_info: Game) -> Self {
        let starting_fen = replay_info
            .tags
            .iter()
            .find(|tag| tag.to_owned().0 == FEN_TAG);
        let active_game_state = match starting_fen {
            Some(fen) => ChessGame::from_fen(&fen.1).unwrap(),
            _ => ChessGame::default(),
        };
        Self {
            active_game: active_game_state,
            replay_info,
            replay_moves_played_offset: 0,
            player_moves_played_offset: 0,
            is_white_turn: true,
        }
    }

    pub fn possible_moves(&self) -> shakmaty::MoveList {
        self.active_game.possible_moves()
    }

    pub fn play_replay_move(&mut self) -> ReplayResponse {
        let mut comment: Option<String> = None;
        let mut last_move_from: Option<Square> = None;
        let mut last_move_to: Option<Square> = None;
        if self.replay_moves_played_offset + 1 <= self.replay_info.moves.len()
            && self.player_moves_played_offset == 0
        {
            let played_move: GameMove =
                self.replay_info.moves[self.replay_moves_played_offset].clone();
            comment = played_move.comment;
            let played_move = played_move.move_.move_;
            let color = if self.is_white_turn {
                ShakmColor::White
            } else {
                ShakmColor::Black
            };
            let played_piece = match &played_move {
                Move::BasicMove { piece, .. } => to_shakmaty_piece(piece, color),
                _ => ShakmPiece {
                    color,
                    role: Role::King,
                },
            };
            let destination = match &played_move {
                Move::BasicMove { to, .. } => ShakmSquare::from_coords(
                    to_shakmaty_file(to.file()).unwrap(),
                    to_shakmaty_rank(to.rank()).unwrap(),
                ),
                Move::CastleKingside => {
                    if self.is_white_turn {
                        ShakmSquare::from_coords(ShakmFile::H, ShakmRank::First)
                    } else {
                        ShakmSquare::from_coords(ShakmFile::H, ShakmRank::Eighth)
                    }
                }
                Move::CastleQueenside => {
                    if self.is_white_turn {
                        ShakmSquare::from_coords(ShakmFile::A, ShakmRank::First)
                    } else {
                        ShakmSquare::from_coords(ShakmFile::A, ShakmRank::Eighth)
                    }
                }
            };
            let (src_col, src_row) = match &played_move {
                Move::BasicMove { from, .. } => {
                    (to_shakmaty_file(from.file()), to_shakmaty_rank(from.rank()))
                }
                Move::CastleKingside => (
                    Some(ShakmFile::E),
                    if self.is_white_turn {
                        Some(ShakmRank::First)
                    } else {
                        Some(ShakmRank::Eighth)
                    },
                ),
                Move::CastleQueenside => (
                    Some(ShakmFile::E),
                    if self.is_white_turn {
                        Some(ShakmRank::First)
                    } else {
                        Some(ShakmRank::Eighth)
                    },
                ),
            };
            match self.active_game.move_piece_by_type(
                played_piece,
                Square::from(destination),
                src_col,
                src_row,
            ) {
                Ok((src, dest)) => {
                    last_move_from = Some(src);
                    last_move_to = Some(dest);
                    self.is_white_turn = !self.is_white_turn;
                    self.replay_moves_played_offset += 1;
                    if self.replay_moves_played_offset == self.replay_info.moves.len() {
                        let termination_string =
                            termination_string_from(self.replay_info.termination);
                        let mut last_move_comment = comment.unwrap_or_default();
                        last_move_comment.push_str(termination_string);
                        comment = Some(last_move_comment);
                    }
                }
                Err(_) => {
                    comment = Some(
                        "Error playing replay move, please check your PGN's validity".into(),
                    )
                }
            }
        } else if self.player_moves_played_offset > 0 {
            comment = Some("Undo Manual Moves before proceeding with replay".into())
        }
        ReplayResponse {
            fen: self.active_game.fen(),
            comment,
            last_move_from,
            last_move_to,
        }
    }

    pub fn player_move(&mut self, source: Square, destination: Square) -> ReplayResponse {
        match self.active_game.move_piece(source, destination) {
            Ok(_) => {
                self.player_moves_played_offset += 1;
            }
            Err(_) => {}
        }
        ReplayResponse {
            fen: self.active_game.fen(),
            comment: None,
            last_move_from: Some(source),
            last_move_to: Some(destination),
        }
    }

    pub fn undo_move(&mut self) -> ReplayResponse {
        if self.player_moves_played_offset > 0 {
            self.active_game.undo(1).ok();
            self.player_moves_played_offset -= 1;
        } else if self.replay_moves_played_offset > 0 {
            self.active_game.undo(1).ok();
            self.replay_moves_played_offset -= 1;
            self.is_white_turn = !self.is_white_turn;
        }
        ReplayResponse {
            fen: self.active_game.fen(),
            comment: None,
            last_move_from: None,
            last_move_to: None,
        }
    }

    pub fn reset(&mut self) -> ReplayResponse {
        self.active_game = ChessGame::default();
        self.replay_moves_played_offset = 0;
        self.player_moves_played_offset = 0;
        self.is_white_turn = true;
        ReplayResponse {
            fen: self.active_game.fen(),
            comment: None,
            last_move_from: None,
            last_move_to: None,
        }
    }
}

fn to_shakmaty_piece(piece: &LocalPiece, color: ShakmColor) -> ShakmPiece {
    let role = match piece {
        LocalPiece::Pawn => Role::Pawn,
        LocalPiece::Knight => Role::Knight,
        LocalPiece::Bishop => Role::Bishop,
        LocalPiece::Rook => Role::Rook,
        LocalPiece::Queen => Role::Queen,
        LocalPiece::King => Role::King,
    };
    ShakmPiece { color, role }
}

fn to_shakmaty_rank(rank: Option<LocalRank>) -> Option<ShakmRank> {
    match rank {
        Some(LocalRank::R1) => Some(ShakmRank::First),
        Some(LocalRank::R2) => Some(ShakmRank::Second),
        Some(LocalRank::R3) => Some(ShakmRank::Third),
        Some(LocalRank::R4) => Some(ShakmRank::Fourth),
        Some(LocalRank::R5) => Some(ShakmRank::Fifth),
        Some(LocalRank::R6) => Some(ShakmRank::Sixth),
        Some(LocalRank::R7) => Some(ShakmRank::Seventh),
        Some(LocalRank::R8) => Some(ShakmRank::Eighth),
        _ => None,
    }
}

fn to_shakmaty_file(file: Option<LocalFile>) -> Option<ShakmFile> {
    match file {
        Some(LocalFile::A) => Some(ShakmFile::A),
        Some(LocalFile::B) => Some(ShakmFile::B),
        Some(LocalFile::C) => Some(ShakmFile::C),
        Some(LocalFile::D) => Some(ShakmFile::D),
        Some(LocalFile::E) => Some(ShakmFile::E),
        Some(LocalFile::F) => Some(ShakmFile::F),
        Some(LocalFile::G) => Some(ShakmFile::G),
        Some(LocalFile::H) => Some(ShakmFile::H),
        _ => None,
    }
}

fn termination_string_from(term_info: GameTermination) -> &'static str {
    match term_info {
        GameTermination::WhiteWins => " Game Over: White Won",
        GameTermination::BlackWins => " Game Over: Black Won",
        GameTermination::DrawnGame => " Game Over: Draw",
        GameTermination::Unknown => " Game Over: Unknown",
    }
}
