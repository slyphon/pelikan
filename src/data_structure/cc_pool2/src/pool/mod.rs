use failure;
use std;
use std::collections::VecDeque;
use std::rc::Rc;
use std::result;

pub(crate) type Result<T> = result::Result<T, failure::Error>;
pub(crate) type ObjectInitFnPtr = unsafe extern "C" fn(*mut u8);

pub trait BufCallback {
    fn callback(&self, buf: &mut [u8]);
}

struct ClosureWrapper(Box<Fn(&mut [u8])>);

impl ClosureWrapper {
    fn new<T: Fn(&mut [u8]) + 'static>(f: T) -> Rc<BufCallback> {
        Rc::new(ClosureWrapper(Box::new(f)))
    }
}

impl BufCallback for ClosureWrapper {
    fn callback(&self, buf: &mut [u8]) {
        (self.0)(&mut buf[..])
    }
}


#[repr(C)]
pub struct FnWrapper(ObjectInitFnPtr);

impl FnWrapper {
    pub fn new(f: ObjectInitFnPtr) -> Self { FnWrapper(f) }
}

impl BufCallback for FnWrapper {
    fn callback(&self, buf: &mut [u8]) {
        unsafe { (self.0)(buf.as_mut_ptr()) }
    }
}


// we can either have a VecDeque of Box<[u8]>, which is like an array
// of (bstring *), or we could contiguously allocate a Vec<u8> and carve
// off owned ranges of it. This implementation follows the existing one, using
// a queue that points to non-contiguous blocks of memory. It's left as an
// enhancement to do the contiguous block implementation.
pub struct Pool {
    freeq: VecDeque<Box<[u8]>>,
    obj_size: usize,
    nused: usize,
    nmax: usize,
    initfn: Rc<BufCallback>,
}

// |<----------- nmax ---------->|
// | nused | freeq     |  slack  |

impl Pool {
    pub fn new(obj_size: usize, nmax: usize, initfn: Rc<BufCallback>) -> Pool {
        Pool{
            freeq: VecDeque::with_capacity(nmax),
            nused: 0,
            nmax:
                match nmax {
                    0 => std::usize::MAX,
                    _ => nmax,
                },
            obj_size,
            initfn,
        }
    }

    pub fn prealloc(&mut self, size: usize) {
        // this doesn't check nmax?
        while self.freeq.len() < size {
            let v = self.allocate_one();
            self.freeq.push_back(v);
        }
    }

    fn allocate_one(&mut self) -> Box<[u8]> {
        let mut bs = vec![0u8; self.obj_size].into_boxed_slice();
        self.initfn.callback(&mut bs[..]);
        bs
    }

    #[inline]
    pub fn take(&mut self) -> Option<Box<[u8]>> {
        let item =
            self.freeq
                .pop_front()
                .or_else(|| {
                    if self.nused < self.nmax {
                        Some(self.allocate_one())
                    } else {
                        None    // we are over capacity
                    }
                });

        if item.is_some() {
            self.nused += 1;
        }
        item
    }

    #[inline]
    pub fn put(&mut self, item: Box<[u8]>) {
        self.freeq.push_back(item);
        self.nused -= 1;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_prealloc_and_alloc_and_new() {
        let objsz = 5;
        let nmax = 10;
        let mut p = Pool::new(objsz, nmax, ClosureWrapper::new(|buf| buf[0] = 1u8 ));

        assert_eq!(p.nused, 0);
        assert_eq!(p.nmax, 10);
        assert_eq!(p.freeq.len(), 0);
        assert!(p.freeq.capacity() >= 10);

        p.prealloc(3);
        assert_eq!(p.freeq.len(), 3);

        // make sure the callback was called
        for b in p.freeq.iter() {
            assert_eq!(b.len(), objsz);
            assert_eq!(b[0], 1u8)
        }
    }

    #[test]
    fn test_borrow_and_unborrow() {
        let objsz = 5;
        let nmax = 2;
        let mut p = Pool::new(objsz, nmax, ClosureWrapper::new(|buf| buf[0] = 1u8 ));

        p.prealloc(1);

        let a = p.take().unwrap();
        let b = p.take().unwrap();    // this should allocate because we're still under nmax
        assert_eq!(p.nused, 2);
        assert!(p.take().is_none());  // sorry we're full

        p.put(a);
        assert_eq!(p.nused, 1);
        p.put(b);
        assert_eq!(p.freeq.len(), 2);
        assert_eq!(p.nused, 0);
    }
}
