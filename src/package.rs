use std::{
  cell::UnsafeCell,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};

pub struct Package<T> {
  is_owner: AtomicBool,
  data: UnsafeCell<T>,
}

fn package<T>(initial: T) -> (PackageOwner<T>, PackageBorrower<T>) {
  let package = Arc::new(Package {
    is_owner: AtomicBool::new(true),
    data: UnsafeCell::new(initial),
  });
  (
    PackageOwner {
      package: package.clone(),
    },
    PackageBorrower { package },
  )
}

pub struct PackageOwner<T> {
  package: Arc<Package<T>>,
}

impl<T> PackageOwner<T> {
  fn is_owned(&self) -> bool {
    self.package.is_owner.load(Ordering::Acquire)
  }

  async fn get(&self) -> &T {
    todo!()
  }

  fn turnover(&self) {}
}

pub struct PackageBorrower<T> {
  package: Arc<Package<T>>,
}

impl<T> PackageBorrower<T> {
  fn is_owned(&self) -> bool {
    self.package.is_owner.load(Ordering::Acquire)
  }
}
