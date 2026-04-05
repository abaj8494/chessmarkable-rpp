use anyhow::Result;
use serde::{Deserializer, Serializer};
use serde_string_derive::SerdeDisplayFromStr;
use shakmaty::{File as ShakmFile, Rank as ShakmRank, Square as ShakmSquare};
use std::fmt;
use thiserror::Error;

const FILES: &[char] = &['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'];
const RANKS: &[char] = &['1', '2', '3', '4', '5', '6', '7', '8'];

#[derive(Error, Debug)]
pub enum SquareFormatError {
    #[error("The square in not within the acceptable range (found: {found}, expected: between A1 and H8)")]
    OutOfRange { found: String },
    #[error("The square has to contain two chars (found: {found}, expected: a value with two chars in range A1 to H8)")]
    InvalidLength { found: String },
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug, SerdeDisplayFromStr)]
#[expected_data_description = "a chess square donated with a letter and a number (e.g. A1)"]
pub struct Square(pub ShakmSquare);

impl Square {
    pub fn new(x: usize, y: usize) -> Result<Self> {
        let file = ShakmFile::ALL.get(x).ok_or_else(|| anyhow!("Invalid File index for pos"))?;
        let rank = ShakmRank::ALL.get(y).ok_or_else(|| anyhow!("Invalid Rank index for pos"))?;
        Ok(Square(ShakmSquare::from_coords(*file, *rank)))
    }

    pub fn x(&self) -> u8 {
        self.0.file() as u8
    }

    pub fn y(&self) -> u8 {
        self.0.rank() as u8
    }

    pub fn inner(&self) -> ShakmSquare {
        self.0
    }
}

impl From<ShakmSquare> for Square {
    fn from(sq: ShakmSquare) -> Self {
        Square(sq)
    }
}

impl From<Square> for ShakmSquare {
    fn from(sq: Square) -> ShakmSquare {
        sq.0
    }
}

impl std::str::FromStr for Square {
    type Err = SquareFormatError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let chars: Vec<_> = s.chars().collect();
        if chars.len() != 2 {
            return Err(SquareFormatError::InvalidLength {
                found: s.to_owned(),
            });
        }

        let file_index = match FILES.iter().position(|f| f == &chars[0]) {
            Some(pos) => pos,
            None => {
                return Err(SquareFormatError::OutOfRange {
                    found: s.to_owned(),
                })
            }
        };

        let rank_index = match RANKS.iter().position(|f| f == &chars[1]) {
            Some(pos) => pos,
            None => {
                return Err(SquareFormatError::OutOfRange {
                    found: s.to_owned(),
                })
            }
        };

        Ok(Square::new(file_index, rank_index).unwrap())
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            FILES[self.x() as usize],
            RANKS[self.y() as usize]
        )
    }
}
