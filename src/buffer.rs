//封装一个buffer接口，api来自我的go项目

#[cfg(test)]
const TEST_MAX_BUF_SIZE: usize = 1 * 16; //清理失效数据阈值
#[cfg(test)]
const TEST_DEFAULT_BUF_SIZE: usize = 1 * 8; //初始化大小

pub(crate) const MAX_BUF_SIZE: usize = 1024 * 64; //清理失效数据阈值
pub(crate) const DEFAULT_BUF_SIZE: usize = 1024 * 8; //初始化大小

use std::fmt;
use std::marker::PhantomPinned;
use std::pin::Pin;

pub struct MsgBufferStatic {
    inner: Pin<Box<MsgBuffer>>,
    pub(crate) ptr: *mut MsgBuffer,
}
impl Drop for MsgBufferStatic {
    fn drop(&mut self) {
        //POOL.put(self)
    }
}
unsafe impl Send for MsgBufferStatic {}
unsafe impl Sync for MsgBufferStatic {}

//Box<[u8]>实现
pub struct MsgBuffer {
    b: Box<[u8]>, //指向raw或者其他地方的裸指针
    max_buf_size: usize,
    //default_buf_size: usize,
    pos: usize, //起点位置
    l: usize,   //终点,长度以b.len()替代
    _pin: PhantomPinned,
}
impl Default for MsgBuffer {
    fn default() -> Self {
        MsgBuffer::new()
    }
}

impl fmt::Display for MsgBufferStatic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, " {} - {}", self.inner.pos, self.inner.l)
    }
}
impl MsgBuffer {
    pub fn new() -> MsgBuffer {
        let v: Vec<u8> = vec![0; DEFAULT_BUF_SIZE];
        let buf = MsgBuffer {
            b: v.into_boxed_slice(),
            max_buf_size: MAX_BUF_SIZE,
            //default_buf_size: DEFAULT_BUF_SIZE,
            pos: 0,
            l: 0,
            _pin: PhantomPinned,
        };
        buf
    }
    pub fn new_with_param(default_buf_size: usize, _max_buf_size: usize) -> MsgBuffer {
        let v: Vec<u8> = vec![0; default_buf_size];
        let buf = MsgBuffer {
            b: v.into_boxed_slice(),
            max_buf_size: default_buf_size,
            //default_buf_size: max_buf_size,
            pos: 0,
            l: 0,
            _pin: PhantomPinned,
        };
        buf
    }

    #[cfg(test)]
    fn test_new() -> Self {
        let v: Vec<u8> = vec![0; TEST_DEFAULT_BUF_SIZE];
        let buf = MsgBuffer {
            b: v.into_boxed_slice(),
            max_buf_size: TEST_MAX_BUF_SIZE,
            //default_buf_size: TEST_DEFAULT_BUF_SIZE,
            pos: 0,
            l: 0,

            _pin: Default::default(),
        };
        buf
    }
    fn grow(&mut self, mut n: usize) {
        unsafe {
            n += self.b.len();
            let len = self.b.len() * 2;
            if len > n {
                let mut v = vec![0; len];
                std::ptr::copy(
                    self.b[self.pos..].as_ptr(),
                    v.as_mut_ptr(),
                    self.b.len() - self.pos,
                );
                self.b = v.into_boxed_slice();
            } else {
                let mut v = vec![0; n];
                std::ptr::copy(
                    self.b[self.pos..].as_ptr(),
                    v.as_mut_ptr(),
                    self.b.len() - self.pos,
                );
                self.b = v.into_boxed_slice();
            }
            self.l -= self.pos;
            self.pos = 0;
        }
    }
    pub fn reset(&mut self) {
        if self.pos > self.max_buf_size {
            #[cfg(test)]
            println!("reset re_size");
            self.re_size();
        }
        self.pos = 0;
        self.l = 0;
    }

    pub fn make(&mut self, l: usize) -> &mut [u8] {
        if self.pos > self.max_buf_size {
            self.re_size();
        }
        self.l += l;

        if self.l > self.b.len() {
            self.grow(l);
            //self.b.reserve(l);
        }

        //unsafe { self.b.set_len(newlen) }

        return &mut self.b[self.l - l..self.l];
    }
    pub fn spare(&mut self, l: usize) -> &mut [u8] {
        if self.pos > self.max_buf_size {
            self.re_size();
        }
        let newlen = self.l + l;

        if newlen > self.b.len() {
            self.grow(l);
        }
        return &mut self.b[self.l..newlen];
    }
    pub unsafe fn string(&self) -> String {
         return String::from_utf8_unchecked(self.bytes().to_vec())
    }
    //重新整理内存
    fn re_size(&mut self) {
        unsafe {
            if self.pos >= self.max_buf_size {
                let len = self.l - self.pos; //原始长度
                let newlen = if len > self.max_buf_size {
                    len
                } else {
                    self.max_buf_size
                };
                let mut v = vec![0; newlen];
                let dst = v.as_mut_ptr();
                std::ptr::copy(self.b[self.pos..].as_ptr(), dst, self.b.len() - self.pos);
                self.l = len;

                self.b = v.into_boxed_slice();
                self.pos = 0;
            }
        }
    }

    pub fn write(&mut self, b: &[u8]) {
        if self.pos > self.max_buf_size {
            self.re_size()
        }

        self.l += b.len();
        unsafe {
            if self.l > self.b.len() {
                self.grow(b.len());
            }

            std::ptr::copy(b.as_ptr(), self.b[self.l - b.len()..].as_mut_ptr(), b.len());
        }

        //self.b.extend_from_slice(b);
    }

    pub fn len(&self) -> usize {
        return self.l - self.pos;
    }
    pub fn shift(&mut self, len: usize) {
        if len <= 0 {
            return;
        }

        if len < self.len() {
            self.pos += len;
            if self.pos > self.max_buf_size {
                self.re_size();
            }
            //println!("{}",self.pos);
        } else {
            self.reset();
        }
    }
    pub fn as_slice(&self) -> &[u8] {
        return &self.b[self.pos..self.l];
    }
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        return &mut self.b[self.pos..self.l];
    }
    pub fn bytes(&self) -> &[u8] {
        return &self.b[self.pos..self.l];
    }

    pub fn write_string(&mut self, s: &str) {
        self.write(s.as_bytes())
    }
    pub fn write_byte(&mut self, s: u8) {
        if self.pos > self.max_buf_size {
            self.re_size();
        }

        self.l += 1;
        if self.l > self.b.len() {
            self.grow(1)
        }

        self.b[self.l - 1] = s;
    }
    pub fn truncate(&mut self, len: usize) {
        let new = self.pos + len;
        if new <= self.b.len() {
            self.l = new;
        } else {
            self.l = self.b.len()
        }
    }
}
impl From<Vec<u8>> for MsgBuffer {
    fn from(value: Vec<u8>) -> Self {
        let l = value.len();
        MsgBuffer {
            b: value.into_boxed_slice(),
            max_buf_size: MAX_BUF_SIZE,
            pos: 0,
            l: l,
            _pin: Default::default(),
        }
    }
}
pub struct MsgBufferPool {
    vec_pool: Vec<Pin<Box<MsgBuffer>>>,
    max_buf_num: usize,
    max_buf_size: usize,
    default_buf_size: usize,
}

impl MsgBufferPool {
    pub fn new() -> Self {
        MsgBufferPool {
            vec_pool: vec![],
            max_buf_num: 64,
            max_buf_size: MAX_BUF_SIZE,
            default_buf_size: DEFAULT_BUF_SIZE,
        }
    }
    pub fn with_max_buf_num(mut self, size: usize) -> Self {
        self.max_buf_num = size;
        self
    }
    pub fn with_max_buf_size(mut self, size: usize) -> Self {
        self.max_buf_size = size;
        self
    }
    pub fn with_default_buf_size(mut self, size: usize) -> Self {
        self.default_buf_size = size;
        self
    }
    pub fn get(&mut self) -> Pin<Box<MsgBuffer>> {
        match self.vec_pool.pop() {
            Some(inner) => inner,
            None => {
                let buf = MsgBuffer::new_with_param(self.default_buf_size, self.max_buf_size);
                Box::pin(buf)
            }
        }
    }
    pub fn put(&mut self, inner: Pin<Box<MsgBuffer>>) {
        if self.vec_pool.len() < self.max_buf_num {
            self.vec_pool.push(inner)
        }
    }
}
impl Iterator for MsgBuffer {
    // we will be counting with usize
    type Item = u8;

    // next() is the only required method
    fn next(&mut self) -> Option<Self::Item> {
        // Increment our count. This is why we started at zero.
        if self.pos == self.l {
            self.reset();
            return None;
        };
        self.pos += 1;
        Some(self.b[self.pos - 1])
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if self.pos + n > self.l {
            self.reset();
            return None;
        };
        self.pos += n + 1;
        Some(self.b[self.pos - 1])
    }
}

impl std::io::Write for MsgBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl MsgBufferStatic {
    pub fn new() -> MsgBufferStatic {
        let inner = MsgBuffer::new();
        let mut buf = MsgBufferStatic {
            inner: Box::pin(inner),
            ptr: std::ptr::null_mut(),
        };
        unsafe {
            buf.ptr = buf.inner.as_mut().get_unchecked_mut();
        }
        buf
    }
    pub fn print(&self) {
        println!("{:}", self);
    }
    pub fn spare(&self, l: usize) -> &mut [u8] {
        unsafe { (*self.ptr).spare(l) }
    }
    pub fn len(&self) -> usize {
        unsafe { (*self.ptr).len() }
    }
    pub fn truncate(&self, len: usize) {
        unsafe { (*self.ptr).truncate(len) }
    }
    pub fn as_slice(&self) -> &[u8] {
        unsafe { (*self.ptr).as_slice() }
    }
    pub fn shift(&self, len: usize) {
        unsafe { (*self.ptr).shift(len) }
    }

    /*pub fn new() -> MsgBuffer {

    }
    pub fn new_with_param(default_buf_size:usize,max_buf_size:usize) -> MsgBuffer {

    }


    fn grow(&mut self, n: usize) {

    }
    pub fn reset(&mut self) {

    }

    pub fn make(&mut self, l: usize) -> &mut [u8] {

    }

    pub fn string(&self)->String{


    }
    //重新整理内存
    fn re_size(&mut self) {

    }

    pub fn write(&mut self, b: &[u8]) {

    }

    pub fn len(&self) -> usize {

    }


    pub fn as_mut_slice(&mut self) -> &mut [u8] {



    }
    pub fn bytes(&self) -> &[u8] {



    }
    pub fn write_string(&mut self, s: &str) {

    }
    pub fn write_byte(&mut self, s: u8) {



    }
    pub fn truncate(&mut self, len: usize) {

    }*/
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msgbuffer() {
        let mut msg = MsgBuffer::test_new();

        let b = msg.make(1);
        assert_eq!(1, b.len());
        let b = msg.make(8);
        b[7] = 0;
        assert_eq!(8, b.len());
        assert_eq!(9, msg.len());
        msg.shift(1);
        assert_eq!(1, msg.pos);
        assert_eq!(8, msg.len());
        msg.shift(7);
        msg.write(&[1, 2, 3]);
        assert_eq!(8, msg.pos);
        assert_eq!(&[0, 1, 2, 3], msg.bytes());
        msg.shift(5);
        assert_eq!(0, msg.pos);
        assert_eq!(msg.bytes(), &[]);
        msg.write(&[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31,
        ]);
        msg.shift(30);
        assert_eq!(0, msg.pos);
        assert_eq!(&[31], msg.bytes());
        msg.write(&[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31,
        ]);
        msg.truncate(2);
        println!("{:?}", msg.bytes());
        msg.write_byte(1);
        msg.write_byte(2);
        assert_eq!(&[31, 1, 1, 2], msg.as_mut_slice());
        msg.truncate(4);
        assert_eq!(&[31, 1, 1, 2], msg.as_slice());
        msg.write_string("123");
        assert_eq!(&[31, 1, 1, 2, 49, 50, 51], msg.as_slice());
    }
}
