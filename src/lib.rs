#![feature(async_closure)]
#![feature(trait_alias)]
#![feature(fn_traits)]
use crate::afterfunc::AfterFn;
use crate::buffer::MsgBufferPool;


use crate::conn::*;
use ::tokio::macros::support::Poll::{Pending, Ready};
use tokio::sync::mpsc::{Receiver, Sender};

use async_trait::async_trait;

use err::NetError;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::u32::MAX;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener};
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tokio::time::{Duration, Instant};
use wg::AsyncWaitGroup;


mod afterfunc;
pub mod buffer;
pub mod codec;
pub mod conn;
pub mod err;
pub mod pool;

#[allow(dead_code)]
pub struct Server<T, E,S:ConnBaseTrait> {
    addr: String,
    max_package_size: usize,
    eventhandler: E,
    readtimeout: Duration,
    writetimeout: Duration,
    max_buf_size: usize,
    default_buf_size: usize,
    buf_num: usize,
    exit_tx: Sender<NetError>,
    exit_rx: Receiver<NetError>,
    bufpool: Option<Arc<Mutex<MsgBufferPool>>>,
    conn_map: Arc<Mutex<HashMap<SocketAddr, Conn<T,S>>>>,
    afterfn: Arc<Mutex<AfterFn<T,S>>>,
}
unsafe impl<T, E,S:ConnBaseTrait> Send for Server<T, E,S> {}
impl<T, E,S> Server<T, E,S>
where
    E: EventHandler<T,S> + Send + Copy + Sync + 'static,
    T: Send + 'static,
    S: ConnBaseTrait+ 'static,
{
    /// Server::<HttpAgent, Handler,net::conn::NoSSL>::new_with_codec(
    pub fn new_with_codec(addr: &str, eventhandler: E) -> Server<T, E,S> {
        let (exit_tx, exit_rx) = mpsc::channel::<NetError>(128);

        let conn_map: HashMap<SocketAddr, Conn<T,S>> = Default::default();
        let conn_map = Arc::new(Mutex::new(conn_map));
        let afterfn = Arc::new(Mutex::new(AfterFn::<T,S>::new()));
        Server {
            addr: addr.to_string(),
            max_package_size: MAX as usize,
            eventhandler,
            readtimeout: Duration::from_secs(60),
            writetimeout: Duration::from_secs(60),
            max_buf_size: buffer::MAX_BUF_SIZE,
            default_buf_size: buffer::DEFAULT_BUF_SIZE,
            buf_num: 128,
            exit_tx,
            exit_rx,
            bufpool: None,
            conn_map,
            afterfn,
        }
    }
    //pub fn with_eventhandler(eventhandler: E) {}
    pub fn with_max_package_size(mut self, size: usize) -> Server<T, E,S> {
        self.max_package_size = size;
        self
    }
    pub fn with_readtimeout(mut self, readtimeout: Duration) -> Server<T, E,S> {
        self.readtimeout = readtimeout;
        self
    }
    pub fn with_writetimeout(mut self, writetimeout: Duration) -> Server<T, E,S> {
        self.writetimeout = writetimeout;
        self
    }

    //将会保留 size * max_buf_size
    pub fn with_max_buf_num(mut self, size: usize) -> Server<T, E,S> {
        self.buf_num = size;

        self
    }

    pub fn with_max_buf_size(mut self, size: usize) -> Server<T, E,S> {
        self.max_buf_size = size;
        self
    }
    pub fn with_default_buf_size(mut self, size: usize) -> Server<T, E,S>  {
        self.default_buf_size = size;
        self
    }
    pub async fn start_server(&mut self) -> Result<(), NetError> {
        let addr = self.addr.clone();
        if self.default_buf_size > self.max_buf_size {
            self.default_buf_size = self.max_buf_size
        }
        let afterfn = self.afterfn.clone();
        let bufpool = MsgBufferPool::new()
            .with_max_buf_num(self.buf_num)
            .with_max_buf_size(self.max_buf_size)
            .with_default_buf_size(self.default_buf_size);
        self.bufpool = Some(Arc::new(Mutex::new(bufpool)));
        let conn_map = self.conn_map.clone();

        let listener = TcpListener::bind(&addr).await.unwrap();
        let mut intervalsec = time::interval(Duration::from_secs(1));
        let mut intervalms = time::interval(Duration::from_millis(1));

        unsafe {
            mod __tokio_select_util {
                #[derive(Debug)]
                pub(super) enum Out<_0, _1, _2, _3> {
                    _0(_0),
                    _1(_1),
                    _2(_2),
                    _3(_3),
                }
            }
            loop {
                let output = {
                    ::tokio::macros::support::poll_fn(|cx| {
                        if let Ready(out) = listener.poll_accept(cx) {
                            return Ready(__tokio_select_util::Out::_0(out));
                        }
                        let f1 = &mut self.exit_rx.recv();
                        if let Ready(out) = Future::poll(Pin::new_unchecked(f1), cx) {
                            return Ready(__tokio_select_util::Out::_1(out));
                        }
                        if let Ready(out) = intervalsec.poll_tick(cx) {
                            return Ready(__tokio_select_util::Out::_2(out));
                        }
                        if let Ready(out) = intervalms.poll_tick(cx) {
                            return Ready(__tokio_select_util::Out::_3(out));
                        }
                        Pending
                    })
                    .await
                };

                match output {
                    __tokio_select_util::Out::_0(accept) => {
                        let (stream, addr) = accept?;
                        self.start_conn_handle(conn::Stream::Tcp(stream), addr, None).await;
                    }
                    __tokio_select_util::Out::_1(e) => match e {
                        Some(e) => return Err(e),
                        None => {
                            println!("server exit channel err ");
                            return Err(NetError::ShutdownServer("".to_string()));
                        }
                    },

                    __tokio_select_util::Out::_3(now) => {
                        let mut afterfn = afterfn.lock().await;
                        afterfn.check_delete(now);
                        if let Some((addr, id)) = afterfn.get_taskaddr_from_time(now) {
                            if let Some(conn) = conn_map.lock().await.get_mut(&addr) {
                                conn.react_tx.try_send(ReactOperationChannel {
                                    op: ReactOperation::Afterfn(id),
                                    res: None,
                                })?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    pub async fn start_conn_handle(
        &mut self,
        stream: Stream<S>,
        addr: SocketAddr,
        callback: Option<Box<dyn FnOnce(T) -> Result<T, NetError> + Send>>,
    ) -> ConnWrite {
        let conn_map = self.conn_map.clone();
        let afterfn = self.afterfn.clone();
        let eventhandler = self.eventhandler;

        let readtimeout = self.readtimeout;

        let max_package_size = self.max_package_size;
        //let (write_tx, mut write_rx) = async_channel::bounded::<OperationChannel>(2048);

        let (react_tx, react_rx) = mpsc::channel::<ReactOperationChannel>(99);
        //let (close_tx, close_rx) = kanal::bounded_async::<Option<String>>(1);

        let conn = Conn::<T,S>::new(
            addr,
            react_tx.clone(),
            afterfn,
            readtimeout,
            stream,
            max_package_size,
        );
        let connwrite = conn.get_write_conn();
        let connid = conn.id;

        /*let mut conn2 = conn.clone();
        {
            let mut map = conn_map.lock().await;
            map.insert(addr, conn);
        }*/
        let wg = AsyncWaitGroup::new();

        //启动1read

        //let readwg = wg.add(1);
        let _react_tx = react_tx.clone();

        //let buf=conn2.readbuf.clone();

        /*let h_handler_readwrite = tokio::spawn(async move {
            if let Err(e) = handler_write(write, write_rx.clone(), _react_tx.clone()).await {
                #[cfg(debug_assertions)]
                println!("read 退出于错误 {:?}", e);
            };
            while let Ok(r) = write_rx.try_recv() {
                if let Some(res) = r.res {
                    res.try_send(OperationResult::Exit);
                }
            }
            readwg.done();

            _react_tx.try_send(ReactOperationChannel {
                op: ReactOperation::Exit,
                res: None,
            });
            #[cfg(debug_assertions)]
            println!("read退出ok{:?}", connid);
        });*/

        //启动react
        let reactwg = wg.add(1);
        //let conn =conn2.clone();
        //let _write_tx = write_tx.clone();

        tokio::spawn(async move {
            if let Err(e) = handler_reactreadwrite(conn, eventhandler, react_rx, callback).await {
                #[cfg(debug_assertions)]
                println!("react 退出于错误 {:?}", e);
            };
            /*while let Ok(Some(r)) = react_rx.try_recv() {
                if let Some(res) = r.res {
                    res.try_send(ReactOperationResult::Exit);
                }
            }*/
            //_write_tx.try_send(OperationChannel {
            //     op: Operation::Exit,
            //    res: None,
            //});
            reactwg.done();
            #[cfg(debug_assertions)]
            println!("react退出ok{:?}", connid);
        });
        //println!("reactok");
        tokio::spawn(async move {
            wg.wait().await;
            #[cfg(debug_assertions)]
            println!("wg退出{:?}", connid);
            //bufpool.lock().await.put(buf1);
            conn_map.lock().await.remove(&addr);
        });
        /*tokio::spawn(async move {
            if let Err(NetError::ShutdownServer(e)) =
                eventhandler.on_closed(&mut ctx, reason).await
            {
                if let Err(_) = conn_exit1.send(NetError::ShutdownServer(e.clone())).await {
                    println!("Server 关闭于错误 {:?}", e)
                }
            };

            conn_exit_tx.try_send(());
        });*/
        return connwrite;
    }
}

//EventHandler后面接具体的ctx类型
#[async_trait]
pub trait EventHandler<T: Send,S:ConnBaseTrait> {
    /// with_param,可以传入一个初始化参数
    /// 返回一个ctx
    async fn on_opened(&mut self, conn: &mut ConnBase<S>) -> Result<T, NetError>;

    /// 返回true，则表示continue，处理下一个消息；false为不是一个完整消息
    /// 例如：收到一个web请求，但是消息未接收完整，返回false，等待下次接收更多消息重新处理
    async fn react(&mut self, conn: &mut ConnBase<S>, ctx: &mut T) -> Result<bool, NetError>;

    async fn on_closed(
        &mut self,
        conn: &mut ConnBase<S>,
        reasion: Option<String>,
    ) -> Result<(), NetError>;
}
