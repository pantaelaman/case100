use crate::core::Environment;

pub struct ExecutorManager {}

pub struct Executor {
  environment: Environment,
}

impl Executor {
  fn new(environment: Environment) -> Self {
    Executor { environment }
  }
}
