use image;
use shakmaty::{Color as ShakmColor, Piece as ShakmPiece, Role};

lazy_static! {

    // Black set
    static ref IMG_KING_BLACK: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/king-black.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_QUEEN_BLACK: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/queen-black.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_BISHOP_BLACK: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/bishop-black.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_ROOK_BLACK: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/rook-black.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_KNIGHT_BLACK: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/knight-black.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_PAWN_BLACK: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/pawn-black.png"))
            .expect("Failed to load resource as image!");

    // White set
    static ref IMG_KING_WHITE: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/king-white.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_QUEEN_WHITE: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/queen-white.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_BISHOP_WHITE: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/bishop-white.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_ROOK_WHITE: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/rook-white.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_KNIGHT_WHITE: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/knight-white.png"))
            .expect("Failed to load resource as image!");
    static ref IMG_PAWN_WHITE: image::DynamicImage =
        image::load_from_memory(include_bytes!("../../../../res/pawn-white.png"))
            .expect("Failed to load resource as image!");
}

pub fn get_orig_piece_img(piece: &ShakmPiece) -> &'static image::DynamicImage {
    match (piece.color, piece.role) {
        (ShakmColor::Black, Role::King) => &IMG_KING_BLACK,
        (ShakmColor::Black, Role::Queen) => &IMG_QUEEN_BLACK,
        (ShakmColor::Black, Role::Bishop) => &IMG_BISHOP_BLACK,
        (ShakmColor::Black, Role::Rook) => &IMG_ROOK_BLACK,
        (ShakmColor::Black, Role::Knight) => &IMG_KNIGHT_BLACK,
        (ShakmColor::Black, Role::Pawn) => &IMG_PAWN_BLACK,
        (ShakmColor::White, Role::King) => &IMG_KING_WHITE,
        (ShakmColor::White, Role::Queen) => &IMG_QUEEN_WHITE,
        (ShakmColor::White, Role::Bishop) => &IMG_BISHOP_WHITE,
        (ShakmColor::White, Role::Rook) => &IMG_ROOK_WHITE,
        (ShakmColor::White, Role::Knight) => &IMG_KNIGHT_WHITE,
        (ShakmColor::White, Role::Pawn) => &IMG_PAWN_WHITE,
    }
}
