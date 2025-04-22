use std::io::Read;

use color_eyre::eyre;
use regex::Regex;

#[derive(Debug)]
pub enum StepFatal {
  Halted,
  AlreadyPoisoned,
  InvalidInstruction { instr: i32 },
  InvalidIAR { iar: u32 },
  InvalidIndex { index: u32 },
  DeviceFailure { error: crate::devices::DeviceError },
  DivisionByZero,
}

pub const MEMORY_SIZE: usize = 16384;

#[derive(Default)]
pub struct StepReport {
  pub changed: Option<u32>,
  pub redraw: bool,
}

#[derive(Clone)]
pub struct Environment {
  pub iar: u32,
  pub memory: Box<[i32; 16384]>,
  poison: bool,
}

impl Default for Environment {
  fn default() -> Self {
    Environment {
      iar: 0,
      memory: Box::new([0; MEMORY_SIZE]),
      poison: false,
    }
  }
}

impl Environment {
  pub fn parse(reader: &mut impl Read) -> eyre::Result<Self> {
    let mut env = Environment::default();
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;

    let regex = Regex::new(r"\s*\d+\s*:\s*(-?\d+);").unwrap();
    for (index, val) in regex
      .captures_iter(&buf)
      .map(|s| s.get(1).unwrap().as_str().parse::<i32>())
      .enumerate()
    {
      env.memory[index] = val?;
    }

    Ok(env)
  }
}

#[tracing::instrument(skip(environment, device_array))]
pub fn step(
  environment: &mut Environment,
  device_array: &mut crate::devices::DeviceArray,
) -> Result<StepReport, StepFatal> {
  let mut report = StepReport::default();

  // if already poisoned, we mustn't do anything here
  if environment.poison {
    return Err(StepFatal::AlreadyPoisoned);
  }

  // if we make it to the end without returning an error, we'll turn this off
  environment.poison = true;

  if environment.iar >= MEMORY_SIZE as u32 - 4 {
    return Err(StepFatal::InvalidIAR {
      iar: environment.iar,
    });
  }

  let [instruction, arg1, arg2, arg3] =
    environment.memory[environment.iar as usize..environment.iar as usize + 4]
  else {
    unreachable!()
  };

  let mut branched = false;

  // log::info!(
  //   "Running instruction {} ({} {} {})",
  //   instruction,
  //   arg1,
  //   arg2,
  //   arg3
  // );

  match instruction {
    0 => return Err(StepFatal::Halted),
    1 | 2 | 3 | 4 | 6 | 7 | 9 | 10 => {
      report.changed = Some(arg1 as u32);
      let arg2v = get_mem(arg2 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg2 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
      let arg3v = get_mem(arg3 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg3 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      let val = match instruction {
        1 => arg2v.wrapping_add(arg3v),
        2 => arg2v.wrapping_sub(arg3v),
        3 => arg2v.saturating_mul(arg3v),
        4 => {
          if arg3v == 0 {
            return Err(StepFatal::DivisionByZero);
          } else {
            arg2v.wrapping_div(arg3v)
          }
        }
        6 => arg2v & arg3v,
        7 => arg2v | arg3v,
        9 => arg2v << arg3v,
        10 => arg2v >> arg3v,
        _ => unreachable!(),
      };

      report.redraw = set_mem(arg1 as u32, val, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg1 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
    }
    // unaries
    5 | 8 => {
      report.changed = Some(arg1 as u32);
      let arg2v = get_mem(arg2 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg2 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      let val = match instruction {
        5 => arg2v,
        8 => !arg2v,
        _ => unreachable!(),
      };

      report.redraw = set_mem(arg1 as u32, val, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg1 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
    }
    // array
    11 | 12 => {
      let arg3v = get_mem(arg3 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg3 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      let index: u32 = (arg2 + arg3v) as u32;

      match instruction {
        11 => {
          report.changed = Some(arg1 as u32);
          let indexv = get_mem(index, environment, device_array)
            .ok_or(StepFatal::InvalidIndex { index: index })?
            .map_err(|error| StepFatal::DeviceFailure { error })?;

          report.redraw =
            set_mem(arg1 as u32, indexv, environment, device_array)
              .ok_or(StepFatal::InvalidIndex { index: arg1 as u32 })?
              .map_err(|error| StepFatal::DeviceFailure { error })?;
        }
        12 => {
          report.changed = Some(index);
          let arg1v = get_mem(arg1 as u32, environment, device_array)
            .ok_or(StepFatal::InvalidIndex { index: arg1 as u32 })?
            .map_err(|error| StepFatal::DeviceFailure { error })?;

          report.redraw =
            set_mem(index as u32, arg1v, environment, device_array)
              .ok_or(StepFatal::InvalidIndex {
                index: index as u32,
              })?
              .map_err(|error| StepFatal::DeviceFailure { error })?;
        }
        _ => unreachable!(),
      }
    }
    // branches
    13 | 14 | 15 => {
      let arg2v = get_mem(arg2 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg2 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
      let arg3v = get_mem(arg3 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg3 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      if match instruction {
        13 => arg2v == arg3v,
        14 => arg2v != arg3v,
        15 => arg2v < arg3v,
        _ => unreachable!(),
      } {
        environment.iar = arg1 as u32;
        branched = true;
      }
    }
    // call
    16 => {
      report.redraw = set_mem(
        arg2 as u32,
        (environment.iar + 4) as i32,
        environment,
        device_array,
      )
      .ok_or(StepFatal::InvalidIndex { index: arg2 as u32 })?
      .map_err(|error| StepFatal::DeviceFailure { error })?;

      report.changed = Some(arg2 as u32);

      environment.iar = arg1 as u32;
      branched = true;
    }
    // ret
    17 => {
      let arg1v = get_mem(arg1 as u32, environment, device_array)
        .ok_or(StepFatal::InvalidIndex { index: arg1 as u32 })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      environment.iar = arg1v as u32;
      branched = true;
    }
    _ => return Err(StepFatal::InvalidInstruction { instr: instruction }),
  }

  if !branched {
    environment.iar += 4;
  }

  environment.poison = false;

  Ok(report)
}

fn get_mem(
  addr: u32,
  environment: &Environment,
  device_array: &mut crate::devices::DeviceArray,
) -> Option<Result<i32, crate::devices::DeviceError>> {
  if addr >= MEMORY_SIZE as u32 {
    device_array.get(addr)
  } else {
    Some(Ok(environment.memory[addr as usize]))
  }
}

fn set_mem(
  addr: u32,
  value: i32,
  environment: &mut Environment,
  device_array: &mut crate::devices::DeviceArray,
) -> Option<Result<bool, crate::devices::DeviceError>> {
  if addr >= MEMORY_SIZE as u32 {
    device_array.set(addr, value)
  } else {
    environment.memory[addr as usize] = value;
    Some(Ok(false))
  }
}
