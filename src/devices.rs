use std::collections::HashMap;

pub mod vga;

#[derive(Default)]
pub struct DeviceArray {
  devices: Vec<Box<dyn DeviceFrame>>,
  registers: HashMap<usize, usize>,
}

impl DeviceArray {
  pub fn register_device(&mut self, device: Box<dyn DeviceFrame>) {
    let idx = self.devices.len();
    self
      .registers
      .extend(device.registers().into_iter().map(|reg| (*reg, idx)));
    self.devices.push(device);
  }

  pub fn set(
    &mut self,
    register: usize,
    value: i32,
  ) -> Option<Result<(), DeviceError>> {
    self
      .registers
      .get(&register)
      .map(|idx| self.devices[*idx].set(register, value))
  }

  pub fn get(&mut self, register: usize) -> Option<Result<i32, DeviceError>> {
    self
      .registers
      .get(&register)
      .map(|idx| self.devices[*idx].get(register))
  }
}

#[derive(Debug)]
pub enum DeviceError {
  Busy,
}

pub trait DeviceFrame: Send {
  fn registers(&self) -> &'static [usize];
  fn set(&mut self, register: usize, value: i32) -> Result<(), DeviceError>;
  fn get(&mut self, register: usize) -> Result<i32, DeviceError>;
}
