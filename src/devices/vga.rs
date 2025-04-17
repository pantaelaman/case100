use color_eyre::eyre;
use sdl3::{
  render::{Canvas, WindowCanvas},
  Sdl,
};

pub struct VgaContext {
  sdl: Sdl,
  canvas: WindowCanvas,
}

impl VgaContext {
  fn init() -> eyre::Result<Self> {
    let sdl_context = sdl3::init()?;
    let video_subsystem = sdl_context.video()?;

    let window = video_subsystem
      .window("VGA Buffer", 640, 480)
      .position_centered()
      .build()
      .unwrap();
    let canvas = window.into_canvas();

    Ok(VgaContext {
      sdl: sdl_context,
      canvas,
    })
  }
}
