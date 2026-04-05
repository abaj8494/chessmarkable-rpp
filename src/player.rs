use shakmaty::Color as ShakmColor;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum Player {
    White = 0,
    Black = 1,
}

impl Player {
    pub fn other_player(&self) -> Self {
        match self {
            Player::Black => Player::White,
            Player::White => Player::Black,
        }
    }
}

impl From<ShakmColor> for Player {
    fn from(color: ShakmColor) -> Self {
        match color {
            ShakmColor::Black => Player::Black,
            ShakmColor::White => Player::White,
        }
    }
}

impl From<Player> for ShakmColor {
    fn from(player: Player) -> ShakmColor {
        match player {
            Player::Black => ShakmColor::Black,
            Player::White => ShakmColor::White,
        }
    }
}

impl std::str::FromStr for Player {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "black" => Ok(Player::Black),
            "white" => Ok(Player::White),
            _ => Err(anyhow!(
                "Specified player is neither \"Black\" nor \"White\""
            )),
        }
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
