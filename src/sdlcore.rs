use color_eyre::eyre;
use sdl3::{render::WindowCanvas, Sdl};
use tokio::{runtime::Builder, sync::mpsc, task::LocalSet};

pub struct SdlExecutor {
  pub _sdl: Sdl,
  pub canvas: WindowCanvas,
  pub draw_cmd_rx: mpsc::Receiver<SdlDrawCommand>,
}

pub struct SdlDrawCommand {
  pub x1: i32,
  pub y1: i32,
  pub x2: i32,
  pub y2: i32,
  pub colour: i32,
}

impl SdlExecutor {
  pub async fn run(draw_cmd_rx: mpsc::Receiver<SdlDrawCommand>) {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    std::thread::spawn(move || -> eyre::Result<()> {
      let local = LocalSet::new();
      local.spawn_local(async move {
        let sdl = sdl3::init()?;
        let video_subsystem = sdl.video()?;

        let window = video_subsystem
          .window("VGA Buffer", 640, 480)
          .position_centered()
          .build()?;

        let mut canvas = window.into_canvas();
        canvas.set_draw_color(sdl3::pixels::Color::BLACK);
        canvas.clear();
        canvas.present();

        let exec = SdlExecutor {
          _sdl: sdl,
          canvas,
          draw_cmd_rx,
        };

        exec.process().await
      });

      rt.block_on(local);
      Ok(())
    });
  }

  async fn process(mut self) -> eyre::Result<()> {
    tracing::info!("Starting SDL process");
    loop {
      tracing::info!("SDL process loop");
      tokio::select! {
        Some(SdlDrawCommand { x1, y1 , x2 , y2 , colour } ) = self.draw_cmd_rx.recv() => {
          tracing::info!("Received draw command {x1} {y1} -- {x2} {y2} ({colour})");
          self.canvas.set_draw_color(value_to_colour(colour));
          self.canvas.fill_rect(Some((x1, y1, (x2 - x1) as u32, (y2 - y1) as u32).into()))?;
          self.canvas.present();
        }
        else => break
      }
    }

    Ok(())
  }
}

fn value_to_colour(value: i32) -> impl Into<sdl3::pixels::Color> {
  (value as u8, (value >> 8) as u8, (value >> 16) as u8)
}
