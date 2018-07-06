extern crate bytes;
extern crate failure;

use pool::{FnWrapper, ObjectInitFnPtr, Pool};
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::raw::c_uchar;
use std::ptr;
use std::rc::Rc;
use std::slice;

mod pool;


#[repr(C)]
pub struct PoolHandle {
    inner: pool::Pool,
    obj_size: usize
}

impl Deref for PoolHandle {
    type Target = pool::Pool;

    fn deref(&self) -> &<Self as Deref>::Target {
        &self.inner
    }
}

impl DerefMut for PoolHandle {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        &mut self.inner
    }
}

#[no_mangle]
pub extern "C" fn pool2_create_handle(
    szof: usize, nmax: u32, initf: ObjectInitFnPtr
) -> *const PoolHandle
{
    let fw = Rc::new(FnWrapper::new(initf));

    let ph = PoolHandle {
        obj_size: szof,
        inner: Pool::new(szof, nmax as usize, fw.clone()),
    };

    Box::into_raw(Box::new(ph))
}

#[no_mangle]
pub extern "C" fn pool2_destroy_handle(handle_p: *mut PoolHandle) {
    drop(unsafe { Box::from_raw(handle_p) });
}

#[no_mangle]
pub extern "C" fn pool2_take(handle_p: *mut PoolHandle) -> *mut c_uchar {
    let mut handle = unsafe { &mut *handle_p };
    let b = handle.take();

    match b {
        Some(bx) => Box::leak(bx).as_mut_ptr(),
        None => ptr::null_mut(),
    }
}


#[no_mangle]
pub extern "C" fn pool2_put(handle_p: *mut PoolHandle, buf_p: *mut c_uchar) {
    let mut handle = unsafe { &mut *handle_p };

    let buf: Box<[u8]> = unsafe {
        Box::from_raw(
            std::slice::from_raw_parts_mut(buf_p, handle.obj_size)
        )
    };

    handle.put(buf);
}
