extern crate bytes;

mod pool;

type ObjectInitFn = unsafe extern "c" fn(*mut u8);

#[repr(C)]
pub struct PoolHandle {
    inner: pool::Pool,
    init_fn: ObjectInitFn,
}

#[no_mangle]
pub extern "C" fn pool2_create_handle(szof: usize, nmax: usize, initf: ObjectInitFn) -> *const PoolHandle {
    pool::Pool::new(szof, nmax)
}

