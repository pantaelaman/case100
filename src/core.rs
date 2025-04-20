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

pub fn step(environment: &mut Environment) -> Result<StepReport, StepFatal> {
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

  log::info!(
    "Running instruction {} ({} {} {})",
    instruction,
    arg1,
    arg2,
    arg3
  );

  match instruction {
    0 => return Err(StepFatal::Halted),
    // binaries
    1 | 2 | 3 | 4 | 6 | 7 | 9 | 10 => {
      report.changed = Some(arg1 as usize);
      let Some(arg2v) = environment.memory.get(arg2 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg2 as usize,
        });
      };
      let Some(arg3v) = environment.memory.get(arg3 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg3 as usize,
        });
      };
      let Some(arg1p) = environment.memory.get_mut(arg1 as usize) else {
        return Err(StepFatal::InvalidIndex {
          index: arg1 as usize,
        });
      };

      match instruction {
        1 => *arg1p = arg2v.wrapping_add(arg3v),
        2 => *arg1p = arg2v.wrapping_sub(arg3v),
        3 => *arg1p = arg2v.saturating_mul(arg3v),
        4 => {
          if arg3v == 0 {
            return Err(StepFatal::DivisionByZero);
          } else {
            *arg1p = arg2v.wrapping_div(arg3v);
          }
        }
        6 => *arg1p = arg2v & arg3v,
        7 => *arg1p = arg2v | arg3v,
        9 => *arg1p = arg2v << arg3v,
        10 => *arg1p = arg2v >> arg3v,
        _ => unreachable!(),
      }
    }
    // unaries
    5 | 8 => {
      report.changed = Some(arg1 as usize);
      let Some(arg2v) = environment.memory.get(arg2 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg2 as usize,
        });
      };
      let Some(arg1p) = environment.memory.get_mut(arg1 as usize) else {
        return Err(StepFatal::InvalidIndex {
          index: arg1 as usize,
        });
      };

      match instruction {
        5 => *arg1p = arg2v,
        8 => *arg1p = !arg2v,
        _ => unreachable!(),
      }
    }
    // array
    11 | 12 => {
      let Some(arg3v) = environment.memory.get(arg3 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg3 as usize,
        });
      };

      let index = arg2 + arg3v;

      match instruction {
        11 => {
          report.changed = Some(arg1 as usize);
          let Some(indexv) = environment.memory.get(index as usize).cloned()
          else {
            return Err(StepFatal::InvalidIndex {
              index: index as usize,
            });
          };
          let Some(arg1p) = environment.memory.get_mut(arg1 as usize) else {
            return Err(StepFatal::InvalidIndex {
              index: arg1 as usize,
            });
          };

          *arg1p = indexv;
        }
        12 => {
          report.changed = Some(index as usize);
          let Some(arg1v) = environment.memory.get(arg1 as usize).cloned()
          else {
            return Err(StepFatal::InvalidIndex {
              index: arg1 as usize,
            });
          };
          let Some(indexp) = environment.memory.get_mut(index as usize) else {
            return Err(StepFatal::InvalidIndex {
              index: index as usize,
            });
          };

          *indexp = arg1v;
        }
        _ => unreachable!(),
      }
    }
    // branches
    13 | 14 | 15 => {
      let Some(arg2v) = environment.memory.get(arg2 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg2 as usize,
        });
      };
      let Some(arg3v) = environment.memory.get(arg3 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg3 as usize,
        });
      };

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
      let Some(arg2p) = environment.memory.get_mut(arg2 as usize) else {
        return Err(StepFatal::InvalidIndex {
          index: arg2 as usize,
        });
      };

      *arg2p = (environment.iar + 4) as i32;
      environment.iar = arg1 as usize;
      branched = true;
    }
    // ret
    17 => {
      let Some(arg1v) = environment.memory.get(arg1 as usize).cloned() else {
        return Err(StepFatal::InvalidIndex {
          index: arg1 as usize,
        });
      };

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
