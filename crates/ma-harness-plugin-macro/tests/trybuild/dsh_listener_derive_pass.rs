//! DshListener derive macro 正确用法 fixture
//!
//! 这个文件应该编译成功 (pass). DshListener 在 Phase 1 是纯 marker (const __DSH_LISTENER),
//! 不引用 seam. trybuild 跑这个验 marker 真的展开.

use ma_harness_plugin_macro::DshListener;

#[derive(DshListener)]
pub struct MyListener;

fn main() {
    let _ = MyListener::__DSH_LISTENER;
}
