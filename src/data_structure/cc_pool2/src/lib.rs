extern crate bytes;
extern crate failure;

use pool::{FnWrapper, ObjectInitFnPtr, Pool};
use std::rc::Rc;

mod pool;


#[repr(C)]
pub struct PoolHandle {
    inner: pool::Pool,
    c_callback: Rc<FnWrapper>,
}

#[no_mangle]
pub extern "C" fn pool2_create_handle(
    szof: usize, nmax: usize, initf: ObjectInitFnPtr
) -> *const PoolHandle
{
    let fw = Rc::new(FnWrapper::new(initf));

    let ph = PoolHandle {
        c_callback: fw.clone(),
        inner: Pool::new(szof, nmax, fw.clone()),
    };

    Box::into_raw(Box::new(ph))
}

