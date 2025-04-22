use std::sync::{
  atomic::{AtomicBool, AtomicU16, Ordering},
  Arc,
};

use tokio::sync::Mutex;

use super::{DeviceError, DeviceFrame};

pub struct LEDDisplayDevice {}

#[derive(Default)]
pub struct HexDisplayDevice {
  pub hex0: Arc<AtomicU16>,
  pub hex1: Arc<AtomicU16>,
}

impl DeviceFrame for HexDisplayDevice {
  fn registers(&self) -> &'static [u32] {
    &[0x80000003, 0x80000004]
  }

  fn set(&mut self, register: u32, value: i32) -> Result<bool, DeviceError> {
    match register {
      0x80000003 => {
        self.hex0.store(value as u16, Ordering::Relaxed);
      }
      0x80000004 => {
        self.hex1.store(value as u16, Ordering::Relaxed);
      }
      _ => unreachable!(),
    }

    Ok(true)
  }

  fn get(&mut self, _register: u32) -> Result<i32, DeviceError> {
    Err(DeviceError::Unreadable)
  }
}

#[derive(Default)]
pub struct LcdDisplayDevice {
  turn: bool,
  x: usize,
  y: usize,
  chr: char,
  pub lcd: Arc<Mutex<[[char; 14]; 2]>>,
}

impl DeviceFrame for LcdDisplayDevice {
  fn registers(&self) -> &'static [u32] {
    &[0x80000010, 0x80000011, 0x80000012, 0x80000013]
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
      0x80000010 => {
        if value == 0 {
          return Err(DeviceError::Unwritable);
        } else {
          let lcd_handle = self.lcd.clone();
          let (x, y, chr) = (self.x, self.y, self.chr);
          tokio::spawn(async move {
            let mut guard = lcd_handle.lock().await;
            guard[y][x] = chr;
          });
        }
      }
      0x80000011 => self.x = value as usize & 0xf,
      0x80000012 => self.y = value as usize & 0x1,
      0x80000013 => self.chr = (value & 0xff) as u8 as char,
      _ => unreachable!(),
    }

    Ok(true)
  }

  fn get(&mut self, register: u32) -> Result<i32, super::DeviceError> {
    match register {
      0x80000010 => Ok(self.turn as i32),
      _ => Err(DeviceError::Unreadable),
    }
  }
}
