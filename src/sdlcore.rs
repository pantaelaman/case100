use color_eyre::eyre;
use sdl3::{
  event::Event, keyboard::Keycode, mouse::MouseButton, render::WindowCanvas,
  Sdl,
};
use tokio::{
  runtime::Builder,
  sync::{mpsc, watch},
  task::LocalSet,
};
use tokio_stream::StreamExt;

pub struct SdlExecutor {
  sdl: Sdl,
  canvas: WindowCanvas,
  pipes: SdlPipesBack,
}

pub fn create_pipes() -> (SdlPipesBack, SdlPipesFront) {
  let (draw_cmd_tx, draw_cmd_rx) = mpsc::channel(10);
  // let (tscr_ev_tx, tscr_ev_rx) = watch::channel(SdlTscrEvent::default());
  let (mouse_ev_tx, mouse_ev_rx) = watch::channel(SdlMouseEvent::default());
  let (kbd_ev_tx, kbd_ev_rx) = watch::channel(SdlKbdEvent::default());
  (
    SdlPipesBack {
      draw_cmd_rx,
      mouse_ev_tx,
      kbd_ev_tx,
    },
    SdlPipesFront {
      draw_cmd_tx,
      mouse_ev_rx,
      kbd_ev_rx,
    },
  )
}

pub struct SdlPipesBack {
  draw_cmd_rx: mpsc::Receiver<SdlDrawCommand>,
  mouse_ev_tx: watch::Sender<SdlMouseEvent>,
  kbd_ev_tx: watch::Sender<SdlKbdEvent>,
}

pub struct SdlPipesFront {
  pub draw_cmd_tx: mpsc::Sender<SdlDrawCommand>,
  pub mouse_ev_rx: watch::Receiver<SdlMouseEvent>,
  pub kbd_ev_rx: watch::Receiver<SdlKbdEvent>,
}

#[derive(Clone)]
pub struct SdlDrawCommand {
  pub x1: i32,
  pub y1: i32,
  pub x2: i32,
  pub y2: i32,
  pub colour: i32,
}

#[derive(Default)]
pub struct SdlTscrEvent {
  pub x: i32,
  pub y: i32,
  pub pressed: bool,
}

#[derive(Clone, Copy)]
pub enum SdlMouseEvent {
  Motion {
    dx: i32,
    dy: i32,
  },
  Button {
    x: i32,
    y: i32,
    down: bool,
    mouse_btn: MouseButton,
  },
}

impl Default for SdlMouseEvent {
  fn default() -> Self {
    SdlMouseEvent::Motion { dx: 0, dy: 0 }
  }
}

#[derive(Clone, Copy)]
pub struct SdlKbdEvent {
  pub down: bool,
  pub keycode: Keycode,
}

impl Default for SdlKbdEvent {
  fn default() -> Self {
    SdlKbdEvent {
      down: false,
      keycode: Keycode::A,
    }
  }
}

impl SdlExecutor {
  pub async fn run(pipes: SdlPipesBack) {
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

        let exec = SdlExecutor { sdl, canvas, pipes };

        exec.process().await
      });

      rt.block_on(local);
      Ok(())
    });
  }

  async fn process(mut self) -> eyre::Result<()> {
    let mut event_pump = self.sdl.event_pump()?;
    let mut event_stream = std::pin::pin! {async_stream::stream! {
      loop {
        if let Some(ev) = event_pump.poll_event() {
          yield ev;
        }
        tokio::task::yield_now().await;
      }
    }};

    tracing::info!("Starting SDL process");
    loop {
      //tracing::info!("SDL process loop");
      tokio::select! {
        Some(SdlDrawCommand { x1, y1 , x2 , y2 , colour } ) = self.pipes.draw_cmd_rx.recv() => {
          tracing::info!("Received draw command {x1} {y1} -- {x2} {y2} ({colour})");
          self.canvas.set_draw_color(value_to_colour(colour));
          self.canvas.fill_rect(Some((x1, y1, (x2 - x1) as u32, (y2 - y1) as u32).into()))?;
          self.canvas.present();
        }
        Some(event) = event_stream.next() => {
          match event {
            Event::MouseButtonDown { mouse_btn, x, y, .. } => {
              self.pipes.mouse_ev_tx.send(
                SdlMouseEvent::Button {
                  mouse_btn,
                  down: true,
                  x: x.round() as i32,
                  y: y.round() as i32,
                }
              )?;
            }
            Event::MouseButtonUp { mouse_btn, x, y, .. } => {
              self.pipes.mouse_ev_tx.send(
                SdlMouseEvent::Button {
                  mouse_btn,
                  down: false,
                  x: x.round() as i32,
                  y: y.round() as i32,
                }
              )?;
            }
            Event::MouseMotion { xrel, yrel, .. } => {
              self.pipes.mouse_ev_tx.send(
                SdlMouseEvent::Motion {
                  dx: xrel.round() as i32,
                  dy: yrel.round() as i32
                }
              )?;
            }
            Event::KeyDown { keycode: Some(keycode), .. } => {
              tracing::info!("Key pressed {:?}", keycode as i32);
              self.pipes.kbd_ev_tx.send(SdlKbdEvent { down: true, keycode })?;
            }
            Event::KeyUp { keycode: Some(keycode), .. } => {
              tracing::info!("Key released {:?}", keycode);
              self.pipes.kbd_ev_tx.send(SdlKbdEvent { down: false, keycode })?;
            }
            _ => {}
          }
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
