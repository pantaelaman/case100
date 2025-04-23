use std::collections::HashMap;

pub mod kbd;
pub mod onboard;
pub mod vga;

#[derive(Default)]
pub struct DeviceArray {
  devices: Vec<Box<dyn DeviceFrame>>,
  registers: HashMap<u32, usize>,
}

impl DeviceArray {
  pub fn register_device(&mut self, device: Box<dyn DeviceFrame>) {
    let idx = self.devices.len();
    self
      .registers
      .extend(device.registers().into_iter().map(|reg| (*reg, idx)));
    self.devices.push(device);
    log::info!("Device array contents: {:?}", self.registers);
  }

  pub fn set(
    &mut self,
    register: u32,
    value: i32,
  ) -> Option<Result<bool, DeviceError>> {
    let register = register & 0xffffffff;
    // log::info!("Seeking to set 0x{:08x} ({register}) to {value}", register);
    // log::info!("Devices {:?}", self.registers);
    // log::info!("Device index {:?}", self.registers.get(&register));
    self
      .registers
      .get(&register)
      .map(|idx| self.devices[*idx].set(register, value))
  }

  pub fn get(&mut self, register: u32) -> Option<Result<i32, DeviceError>> {
    self
      .registers
      .get(&register)
      .map(|idx| self.devices[*idx].get(register))
  }
}

#[derive(Debug)]
pub enum DeviceError {
  Busy,
  Dead,
  Unreadable,
  Unwritable,
}

pub trait DeviceFrame: Send {
  fn registers(&self) -> &'static [u32];
  fn set(&mut self, register: u32, value: i32) -> Result<bool, DeviceError>;
  fn get(&mut self, register: u32) -> Result<i32, DeviceError>;
}
