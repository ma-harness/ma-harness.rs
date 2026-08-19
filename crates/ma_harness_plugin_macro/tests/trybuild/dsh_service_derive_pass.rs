//! DshService derive macro 正确用法 fixture
//!
//! 这个文件应该编译成功 (pass). trybuild 跑 rustc, 期望 0 error.
//! DshService 在 Day 58 改成纯 marker (const __DSH_SERVICE),
//! user 自己手写完整 Service impl. 不引用 seam, 跟 host crate deps 一致.

use ma_harness_cordis::Context;
use ma_harness_plugin_macro::DshService;

#[derive(DshService)]
pub struct MyService;

impl ma_harness_cordis::Service for MyService {
    type Ctx = Context;
    type Error = ma_harness_cordis::BoxedError;
    fn install(_ctx: &Context) -> Result<Self, Self::Error>
    where
        Self: Sized,
        Self::Error: Sized,
    {
        Ok(MyService)
    }
    fn name(&self) -> &str {
        "my_service"
    }
}

fn main() {
    use ma_harness_cordis::Service as _;
    let ctx = Context::new();
    let svc = <MyService as ma_harness_cordis::Service>::install(&ctx).unwrap();
    assert_eq!(svc.name(), "my_service");
    // marker const 存在 (验 macro 真展开了)
    let _ = MyService::__DSH_SERVICE;
}
