use std::io::Read;

use color_eyre::eyre;
use regex::Regex;

#[derive(Debug)]
pub enum StepFatal {
  Halted,
  AlreadyPoisoned,
  InvalidInstruction { instr: i32 },
  InvalidIAR { iar: usize },
  InvalidIndex { index: usize },
  DeviceFailure { error: crate::devices::DeviceError },
  DivisionByZero,
}

pub const MEMORY_SIZE: usize = 16384;

#[derive(Default)]
pub struct StepReport {
  changed: Option<usize>,
}

#[derive(Clone)]
pub struct Environment {
  pub iar: usize,
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

  if environment.iar >= MEMORY_SIZE - 4 {
    return Err(StepFatal::InvalidIAR {
      iar: environment.iar,
    });
  }

  let [instruction, arg1, arg2, arg3] =
    environment.memory[environment.iar..environment.iar + 4]
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
    1 | 2 | 3 | 4 | 6 | 7 | 9 | 10 => {
      report.changed = Some(arg1 as usize);
      let arg2v = get_mem(arg2 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg2 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
      let arg3v = get_mem(arg3 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg3 as usize,
        })?
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

      set_mem(arg1 as usize, val, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg1 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
    }
    // unaries
    5 | 8 => {
      report.changed = Some(arg1 as usize);
      let arg2v = get_mem(arg2 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg2 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      let val = match instruction {
        5 => arg2v,
        8 => !arg2v,
        _ => unreachable!(),
      };

      set_mem(arg1 as usize, val, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg1 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
    }
    // array
    11 | 12 => {
      let arg3v = get_mem(arg3 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg3 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      let index = arg2 + arg3v;

      match instruction {
        11 => {
          report.changed = Some(arg1 as usize);
          let indexv = get_mem(index as usize, environment, device_array)
            .ok_or(StepFatal::InvalidIndex {
              index: index as usize,
            })?
            .map_err(|error| StepFatal::DeviceFailure { error })?;

          set_mem(arg1 as usize, indexv, environment, device_array)
            .ok_or(StepFatal::InvalidIndex {
              index: arg1 as usize,
            })?
            .map_err(|error| StepFatal::DeviceFailure { error })?;
        }
        12 => {
          report.changed = Some(index as usize);
          let arg1v = get_mem(arg1 as usize, environment, device_array)
            .ok_or(StepFatal::InvalidIndex {
              index: arg1 as usize,
            })?
            .map_err(|error| StepFatal::DeviceFailure { error })?;

          set_mem(index as usize, arg1v, environment, device_array)
            .ok_or(StepFatal::InvalidIndex {
              index: index as usize,
            })?
            .map_err(|error| StepFatal::DeviceFailure { error })?;
        }
        _ => unreachable!(),
      }
    }
    // branches
    13 | 14 | 15 => {
      let arg2v = get_mem(arg2 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg2 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;
      let arg3v = get_mem(arg3 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg3 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      if match instruction {
        13 => arg2v == arg3v,
        14 => arg2v != arg3v,
        15 => arg2v < arg3v,
        _ => unreachable!(),
      } {
        environment.iar = arg1 as usize;
        branched = true;
      }
    }
    // call
    16 => {
      set_mem(
        arg2 as usize,
        (environment.iar + 4) as i32,
        environment,
        device_array,
      )
      .ok_or(StepFatal::InvalidIndex {
        index: arg2 as usize,
      })?
      .map_err(|error| StepFatal::DeviceFailure { error })?;

      environment.iar = arg1 as usize;
      branched = true;
    }
    // ret
    17 => {
      let arg1v = get_mem(arg1 as usize, environment, device_array)
        .ok_or(StepFatal::InvalidIndex {
          index: arg1 as usize,
        })?
        .map_err(|error| StepFatal::DeviceFailure { error })?;

      environment.iar = arg1v as usize;
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
  addr: usize,
  environment: &Environment,
  device_array: &mut crate::devices::DeviceArray,
) -> Option<Result<i32, crate::devices::DeviceError>> {
  if addr >= MEMORY_SIZE {
    device_array.get(addr)
  } else {
    Some(Ok(environment.memory[addr]))
  }
}

fn set_mem(
  addr: usize,
  value: i32,
  environment: &mut Environment,
  device_array: &mut crate::devices::DeviceArray,
) -> Option<Result<(), crate::devices::DeviceError>> {
  if addr >= MEMORY_SIZE {
    device_array.set(addr, value)
  } else {
    environment.memory[addr] = value;
    Some(Ok(()))
  }
}
