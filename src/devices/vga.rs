use tokio::sync::mpsc;

use super::{DeviceError, DeviceFrame};

pub struct VgaDevice {
  turn: bool,
  write_mode: bool,
  x1: i32,
  x2: i32,
  y1: i32,
  y2: i32,
  colour: i32,
  draw_cmd_tx: mpsc::Sender<crate::sdlcore::SdlDrawCommand>,
}

impl VgaDevice {
  pub fn new(
    draw_cmd_tx: mpsc::Sender<crate::sdlcore::SdlDrawCommand>,
  ) -> Self {
    Self {
      turn: false,
      write_mode: true,
      x1: 0,
      x2: 0,
      y1: 0,
      y2: 0,
      colour: 0,
      draw_cmd_tx,
    }
  }
}

impl DeviceFrame for VgaDevice {
  fn registers(&self) -> &'static [u32] {
    &[
      0x80000060, 0x80000061, 0x80000062, 0x80000063, 0x80000064, 0x80000065,
      0x80000066,
    ]
  }

  fn set(
    &mut self,
    register: u32,
    value: i32,
  ) -> Result<bool, super::DeviceError> {
    if self.turn {
      return Err(DeviceError::Busy);
    }

    match register {
      0x80000060 => {
        if value == 0 {
          return Err(DeviceError::Unwritable);
        } else {
          if self.write_mode {
            log::info!("Sending draw command");
            self
              .draw_cmd_tx
              .try_send(crate::sdlcore::SdlDrawCommand {
                x1: self.x1,
                x2: self.x2,
                y1: self.y1,
                y2: self.y2,
                colour: self.colour,
              })
              .map_err(|_| DeviceError::Dead)?;
          } else {
            return Err(DeviceError::Dead);
          }
        }
      }
      0x80000061 => {
        self.write_mode = value > 0;
      }
      0x80000062 => {
        self.x1 = value & 0x3ff;
      }
      0x80000063 => {
        self.y1 = value & 0x1ff;
      }
      0x80000064 => {
        self.x2 = value & 0x3ff;
      }
      0x80000065 => {
        self.y2 = value & 0x1ff;
      }
      0x80000066 => {
        self.colour = value & 0xffffff;
      }
      _ => unreachable!(),
    }

    Ok(false)
  }

  fn get(&mut self, register: u32) -> Result<i32, super::DeviceError> {
    match register {
      0x80000060 => Ok(self.turn as i32),
      _ => Err(DeviceError::Unreadable),
    }
  }
}
