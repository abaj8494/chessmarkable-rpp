use crate::game::ChessGame;
pub use crate::game::{move_dst, move_src, ChessOutcome};
use crate::{Player, Square};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Move as ShakmMove, Position, Role, Square as ShakmSquare};
use std::thread;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};

#[derive(Clone, Debug)]
pub struct ChessConfig {
    pub starting_fen: Option<String>,
    pub can_black_undo: bool,
    pub can_white_undo: bool,
    pub allow_undo_after_loose: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChessRequest {
    CurrentBoard,
    CurrentTotalMoves,
    CurrentOutcome,
    MovePiece {
        source: Square,
        destination: Square,
        #[serde(skip)]
        promotion: Option<shakmaty::Role>,
    },
    Abort { message: String },
    UndoMoves { moves: u16 },
}

impl ChessRequest {
    /// Is a spectator allowed to send this request
    pub fn available_to_spectator(&self) -> bool {
        match self {
            ChessRequest::CurrentBoard | ChessRequest::CurrentTotalMoves => true,
            _ => false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChessUpdate {
    Board {
        fen: String,
    },
    PlayerMovedAPiece {
        player: Player,
        moved_piece_source: Square,
        moved_piece_destination: Square,
    },
    PlayerSwitch {
        player: Player,
        fen: String,
    },
    MovePieceFailedResponse {
        message: String,
        fen: String,
    },
    Outcome {
        outcome: Option<ChessOutcome>,
    },
    PossibleMoves {
        possible_moves: Vec<(Square /* From */, Square /* To */)>,
    },
    GenericErrorResponse {
        message: String,
    },
    UndoMovesFailedResponse {
        message: String,
    },
    MovesUndone {
        who: Player,
        moves: u16,
    },
    CurrentTotalMovesReponse {
        total_moves: u16,
    },
}

fn moves_to_square_pairs(game: &ChessGame) -> Vec<(Square, Square)> {
    game.possible_moves()
        .iter()
        .filter_map(|m| {
            let src = move_src(m)?;
            let dst = move_dst(m);
            Some((Square::from(src), Square::from(dst)))
        })
        .collect()
}

pub async fn create_game(
    white: (Sender<ChessUpdate>, Receiver<ChessRequest>),
    black: (Sender<ChessUpdate>, Receiver<ChessRequest>),
    spectators: (Sender<ChessUpdate>, Receiver<ChessRequest>),
    config: ChessConfig,
) -> Result<()> {
    let mut game = if let Some(ref fen) = config.starting_fen {
        ChessGame::from_fen(fen)?
    } else {
        ChessGame::default()
    };

    let (white_tx, white_rx) = white;
    let (black_tx, black_rx) = black;
    let (spectators_tx, spectators_rx) = spectators;

    let (combined_tx, combined_rx) = channel::<(Option<Player>, ChessRequest)>(1024);

    let mut white_rx = ReceiverStream::new(white_rx);
    let mut black_rx = ReceiverStream::new(black_rx);
    let mut spectators_rx = ReceiverStream::new(spectators_rx);
    let mut combined_rx = ReceiverStream::new(combined_rx);

    macro_rules! send_to_everyone {
        ($msg: expr) => {
            white_tx.send($msg.clone()).await.ok();
            black_tx.send($msg.clone()).await.ok();
            spectators_tx.send($msg).await.ok();
        };
    }

    let combined_white_tx = combined_tx.clone();
    task::spawn(async move {
        let player = Some(Player::White);
        loop {
            let update = match white_rx.next().await {
                Some(update) => update,
                None => {
                    combined_white_tx
                        .send((
                            player,
                            ChessRequest::Abort {
                                message: "[Internal] Connection lost".to_owned(),
                            },
                        ))
                        .await
                        .ok();
                    return;
                }
            };
            if combined_white_tx.send((player, update)).await.is_err() {
                return;
            }
        }
    });
    let combined_black_tx = combined_tx.clone();
    task::spawn(async move {
        loop {
            let update = match black_rx.next().await {
                Some(update) => update,
                None => return,
            };
            if combined_black_tx
                .send((Some(Player::Black), update))
                .await
                .is_err()
            {
                return;
            }
        }
    });
    let combined_spectators_tx = combined_tx;
    task::spawn(async move {
        loop {
            let update = match spectators_rx.next().await {
                Some(update) => update,
                None => return,
            };
            if combined_spectators_tx
                .send((None, update))
                .await
                .is_err()
            {
                return;
            }
        }
    });

    // Start
    send_to_everyone!(ChessUpdate::PlayerSwitch {
        player: game.turn(),
        fen: game.fen()
    });
    let possible_moves = moves_to_square_pairs(&game);
    match game.turn() {
        Player::White => white_tx.clone(),
        Player::Black => black_tx.clone(),
    }
    .send(ChessUpdate::PossibleMoves { possible_moves })
    .await
    .ok();

    info!("Game initialized. Handling requests...");

    loop {
        let (sender, request): (Option<Player>, ChessRequest) = match combined_rx.next().await {
            Some(res) => res,
            None => {
                break;
            }
        };

        if sender.is_none() && !request.available_to_spectator() {
            spectators_tx
                .send(ChessUpdate::GenericErrorResponse {
                    message: "Spectators can't send this kind of request!".to_owned(),
                })
                .await
                .ok();
            continue;
        }

        macro_rules! send_to_sender {
            ($msg: expr) => {
                match sender {
                    Some(player) => match player {
                        Player::White => white_tx.send($msg).await.ok(),
                        Player::Black => black_tx.send($msg).await.ok(),
                    },
                    None => spectators_tx.send($msg).await.ok(),
                };
            };
        }

        macro_rules! send_to_other_player {
            ($msg: expr) => {
                match sender.context("Send to the other player")? {
                    Player::White => black_tx.send($msg).await.ok(),
                    Player::Black => white_tx.send($msg).await.ok(),
                };
            };
        }

        match request {
            ChessRequest::CurrentBoard => {
                send_to_sender!(ChessUpdate::Board { fen: game.fen() });
            }
            ChessRequest::CurrentTotalMoves => {
                send_to_sender!(ChessUpdate::CurrentTotalMovesReponse {
                    total_moves: game.total_moves()
                });
            }
            ChessRequest::CurrentOutcome => {
                send_to_sender!(ChessUpdate::Outcome {
                    outcome: game.outcome()
                });
            }
            _ => {}
        }

        let sender = sender
            .context("available_to_spectator() is probably not up to date with the handlers")?;
        match request {
            ChessRequest::MovePiece {
                source,
                destination,
                promotion,
            } => {
                let prev_outcome = game.outcome();
                match game.move_piece(source, destination, promotion) {
                    Ok(_) => {
                        send_to_everyone!(ChessUpdate::PlayerMovedAPiece {
                            player: sender,
                            moved_piece_source: source,
                            moved_piece_destination: destination,
                        });
                        let new_outcome = game.outcome();
                        if prev_outcome != new_outcome {
                            send_to_everyone!(ChessUpdate::Outcome {
                                outcome: new_outcome
                            });
                        }

                        send_to_everyone!(ChessUpdate::PlayerSwitch {
                            player: game.turn(),
                            fen: game.fen(),
                        });

                        if new_outcome.is_none() {
                            let possible_moves = moves_to_square_pairs(&game);
                            send_to_other_player!(ChessUpdate::PossibleMoves { possible_moves });
                        }
                    }
                    Err(e) => {
                        send_to_sender!(ChessUpdate::MovePieceFailedResponse {
                            message: format!("Denied by engine: {}", e),
                            fen: game.fen(),
                        });
                    }
                };
            }
            ChessRequest::Abort { .. } => {
                game.player_left(sender);
                break;
            }
            ChessRequest::UndoMoves { moves } => {
                let player_allowed = match sender {
                    Player::Black => config.can_black_undo,
                    Player::White => config.can_white_undo,
                };
                if !player_allowed {
                    send_to_sender!(ChessUpdate::UndoMovesFailedResponse {
                        message: "You are not permitted to do that in this game.".to_owned(),
                    });
                } else if !(game.turn() == sender && game.outcome().is_none()
                    || game.outcome().is_some() && config.allow_undo_after_loose)
                {
                    if config.allow_undo_after_loose {
                        send_to_sender!(ChessUpdate::UndoMovesFailedResponse {
                            message:
                                "You can only undo when you are playing or it's game over."
                                    .to_owned(),
                        });
                    } else {
                        send_to_sender!(ChessUpdate::UndoMovesFailedResponse {
                            message: "You can only undo when you are playing.".to_owned(),
                        });
                    }
                } else {
                    let prev_outcome = game.outcome();
                    if let Err(e) = game.undo(moves) {
                        send_to_sender!(ChessUpdate::UndoMovesFailedResponse {
                            message: format!("Denied by engine: {}", e),
                        });
                    } else {
                        let new_outcome = game.outcome();
                        if prev_outcome != new_outcome {
                            send_to_everyone!(ChessUpdate::Outcome {
                                outcome: new_outcome
                            });
                        }
                        send_to_everyone!(ChessUpdate::PlayerSwitch {
                            player: game.turn(),
                            fen: game.fen()
                        });
                        let possible_moves = moves_to_square_pairs(&game);
                        match game.turn() {
                            Player::White => white_tx.clone(),
                            Player::Black => black_tx.clone(),
                        }
                        .send(ChessUpdate::PossibleMoves { possible_moves })
                        .await
                        .ok();
                        send_to_everyone!(ChessUpdate::MovesUndone {
                            who: sender,
                            moves,
                        });
                    }
                }
            }
            _ => {
                bail!("Unhandled player-specific request");
            }
        };
    }

    info!("Game terminated seemingly gracefully");
    Ok(())
}

// --- Alpha-Beta Search Bot ---

/// Piece-square tables for positional evaluation (from white's perspective).
/// Values are centipawns bonus for each square.
const PAWN_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

const KNIGHT_TABLE: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50,
    -40,-20,  0,  0,  0,  0,-20,-40,
    -30,  0, 10, 15, 15, 10,  0,-30,
    -30,  5, 15, 20, 20, 15,  5,-30,
    -30,  0, 15, 20, 20, 15,  0,-30,
    -30,  5, 10, 15, 15, 10,  5,-30,
    -40,-20,  0,  5,  5,  0,-20,-40,
    -50,-40,-30,-30,-30,-30,-40,-50,
];

const BISHOP_TABLE: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0, 10, 10, 10, 10,  0,-10,
    -10,  5,  5, 10, 10,  5,  5,-10,
    -10,  0,  5, 10, 10,  5,  0,-10,
    -10, 10, 10, 10, 10, 10, 10,-10,
    -10,  5,  0,  0,  0,  0,  5,-10,
    -20,-10,-10,-10,-10,-10,-10,-20,
];

const ROOK_TABLE: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

fn material_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight => 320,
        Role::Bishop => 330,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 20000,
    }
}

fn piece_square_value(role: Role, sq: ShakmSquare, is_white: bool) -> i32 {
    // Square index: a1=0, b1=1, ..., h8=63
    // Tables are from white's perspective (rank 8 at top = index 0..7)
    let idx = if is_white {
        // Flip: white pieces use table from rank 8 perspective
        (7 - sq.rank() as usize) * 8 + sq.file() as usize
    } else {
        sq.rank() as usize * 8 + sq.file() as usize
    };

    match role {
        Role::Pawn => PAWN_TABLE[idx],
        Role::Knight => KNIGHT_TABLE[idx],
        Role::Bishop => BISHOP_TABLE[idx],
        Role::Rook => ROOK_TABLE[idx],
        _ => 0,
    }
}

fn evaluate(pos: &Chess) -> i32 {
    let board = pos.board();
    let mut score = 0i32;

    for sq in ShakmSquare::ALL {
        if let Some(piece) = board.piece_at(sq) {
            let val = material_value(piece.role) + piece_square_value(piece.role, sq, piece.color == shakmaty::Color::White);
            if piece.color == shakmaty::Color::White {
                score += val;
            } else {
                score -= val;
            }
        }
    }

    // Bonus for mobility
    let moves = pos.legal_moves().len() as i32;
    if pos.turn() == shakmaty::Color::White {
        score += moves * 2;
    } else {
        score -= moves * 2;
    }

    score
}

fn alpha_beta(pos: &Chess, depth: u16, mut alpha: i32, beta: i32, maximizing: bool) -> i32 {
    if depth == 0 || pos.is_game_over() {
        if pos.is_checkmate() {
            return if maximizing { -100000 } else { 100000 };
        }
        if pos.is_stalemate() || pos.is_insufficient_material() {
            return 0;
        }
        return evaluate(pos);
    }

    let moves = pos.legal_moves();

    if maximizing {
        let mut max_eval = i32::MIN;
        for m in moves.iter() {
            let mut new_pos = pos.clone();
            new_pos.play_unchecked(m.clone());
            let eval = alpha_beta(&new_pos, depth - 1, alpha, beta, false);
            max_eval = max_eval.max(eval);
            alpha = alpha.max(eval);
            if beta <= alpha {
                break;
            }
        }
        max_eval
    } else {
        let mut min_eval = i32::MAX;
        for m in moves.iter() {
            let mut new_pos = pos.clone();
            new_pos.play_unchecked(m.clone());
            let eval = alpha_beta(&new_pos, depth - 1, alpha, beta, true);
            min_eval = min_eval.min(eval);
            let new_beta = beta.min(eval);
            if new_beta <= alpha {
                break;
            }
        }
        min_eval
    }
}

pub fn best_move(pos: &Chess, depth: u16) -> Option<ShakmMove> {
    let moves = pos.legal_moves();
    if moves.is_empty() {
        return None;
    }

    let maximizing = pos.turn() == shakmaty::Color::White;
    let mut best: Option<ShakmMove> = None;
    let mut best_eval = if maximizing { i32::MIN } else { i32::MAX };

    for m in moves.iter() {
        let mut new_pos = pos.clone();
        new_pos.play_unchecked(m.clone());
        let eval = alpha_beta(&new_pos, depth - 1, i32::MIN, i32::MAX, !maximizing);

        let is_better = if maximizing {
            eval > best_eval
        } else {
            eval < best_eval
        };

        if is_better {
            best_eval = eval;
            best = Some(m.clone());
        }
    }

    best
}

pub async fn create_bot(
    me: Player,
    depth: u16,
    min_reaction_delay: Duration,
) -> Result<(Sender<ChessUpdate>, Receiver<ChessRequest>)> {
    let (update_tx, mut update_rx) = channel::<ChessUpdate>(256);
    let (request_tx, request_rx) = channel::<ChessRequest>(256);

    task::spawn(async move {
        info!("Bot spawned for {}", me);
        let mut current_outcome: Option<ChessOutcome> = None;
        while let Some(update) = update_rx.recv().await {
            match update {
                ChessUpdate::PlayerSwitch { player, ref fen } => {
                    if player == me && current_outcome.is_none() {
                        let fen_str = fen.clone();
                        let search_depth = depth;

                        let bot_move = task::spawn_blocking(move || {
                            let started = SystemTime::now();

                            let parsed: shakmaty::fen::Fen = fen_str.parse()
                                .expect("Bot failed to parse the provided fen");
                            let pos: Chess =
                                parsed.into_position(shakmaty::CastlingMode::Standard)
                                    .expect("Bot failed to create position from fen");

                            let the_move = best_move(&pos, search_depth);
                            let elapsed = started.elapsed().unwrap_or(Duration::new(0, 0));

                            if elapsed < min_reaction_delay {
                                thread::sleep(min_reaction_delay - elapsed);
                            } else {
                                info!("Bot took a long time to think: {:?}", elapsed);
                            }
                            the_move
                        })
                        .await
                        .context("Blocking heavy calculation")
                        .unwrap();

                        if let Some(m) = bot_move {
                            let src = move_src(&m).unwrap();
                            let dst = move_dst(&m);
                            let promotion = match &m {
                                shakmaty::Move::Normal { promotion, .. } => *promotion,
                                _ => None,
                            };
                            request_tx
                                .send(ChessRequest::MovePiece {
                                    source: Square::from(src),
                                    destination: Square::from(dst),
                                    promotion,
                                })
                                .await
                                .expect("Bot failed to send move");
                        }
                    }
                }
                ChessUpdate::MovePieceFailedResponse { message, .. } => {
                    error!("A move from the bot was rejected: {}", message);
                    break;
                }
                ChessUpdate::Outcome { outcome } => {
                    if outcome.is_some() {
                        info!("Bot detected that the game ended");
                    } else {
                        info!("Game continues. Bot will continue playing.");
                    }
                    current_outcome = outcome;
                }
                _ => {}
            }
        }
        info!("Bot task has ended");
    });

    Ok((update_tx, request_rx))
}

pub fn stubbed_spectator() -> (Sender<ChessUpdate>, Receiver<ChessRequest>) {
    let (update_tx, _) = channel::<ChessUpdate>(1);
    let (_, request_rx) = channel::<ChessRequest>(1);
    (update_tx, request_rx)
}
