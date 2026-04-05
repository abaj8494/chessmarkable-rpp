mod board_select_scene;
mod game_scene;
mod main_menu_scene;
mod pgn_select_scene;
mod replay_scene;
mod piece_images;

pub use board_select_scene::BoardSelectScene;
pub use game_scene::{GameMode, GameScene, SavestateSlot};
pub use main_menu_scene::MainMenuScene;
pub use pgn_select_scene::PgnSelectScene;
pub use replay_scene::ReplayScene;

use crate::canvas::Canvas;
use crate::rmpp_hal::types::InputEvent;
use downcast_rs::Downcast;

pub trait Scene: Downcast {
    fn on_input(&mut self, _event: InputEvent) {}
    fn draw(&mut self, canvas: &mut Canvas);
}
impl_downcast!(Scene);
