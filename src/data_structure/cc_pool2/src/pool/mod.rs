use std::collections::VecDeque;
use std;


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
    initfn: Box<FnMut(&mut Box<[u8]>)>,
}

// |<----------- nmax ---------->|
// | nused | freeq     |  slack  |

impl Pool {
    pub fn new<F>(obj_size: usize, nmax: usize, initfn: F) -> Pool
    where F: FnMut(&mut Box<[u8]>) + 'static
    {
        Pool{
            freeq: VecDeque::with_capacity(nmax),
            nused: 0,
            nmax:
                match nmax {
                    0 => std::usize::MAX,
                    _ => nmax,
                },
            obj_size,
            initfn: Box::new(initfn),
        }
    }

    /// Calculate the 'slack' space we have, which is the total number of
    /// unallocated objects we can grow before we hit `nmax`.
    /// This is `nmax - (freeq.len() + nused)`
    fn slack(&self) -> usize {
        self.nmax - (self.freeq.len() + self.nused)
    }

    pub fn prealloc(&mut self, size: usize) {
        // this doesn't check nmax?
        while self.freeq.len() < size {
            self.freeq.push_back(self.allocate_one());
        }
    }

    fn allocate_one(&mut self) -> Box<[u8]> {
        let mut bs = vec![0u8; self.obj_size].into_boxed_slice();
        self.initfn(&mut bs);
        bs
    }

    pub fn borrow(&mut self) -> Option<Box<[u8]>> {
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

    pub fn unborrow(&mut self, item: Box<[u8]>) {
        self.freeq.push_back(item);
        self.nused -= 1;
    }
}
