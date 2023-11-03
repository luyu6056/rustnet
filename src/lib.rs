#![feature(async_closure)]
#![feature(trait_alias)]
#![feature(fn_traits)]

use crate::afterfunc::AfterFn;
use crate::buffer::MsgBufferPool;

pub use crate::conn::*;

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

use std::sync::atomic::AtomicU64;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
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
pub struct Server<T, E, S: ConnBaseTrait> {
    addr: String,
    max_package_size: usize,
    eventhandler: E,
    readtimeout: Duration,
    writetimeout: Duration,
    max_buf_size: usize,
    default_buf_size: usize,
    buf_num: usize,
    bufpool: Option<Arc<Mutex<MsgBufferPool>>>,
    conn_map: Arc<Mutex<HashMap<SocketAddr, Sender<ReactOperationChannel>>>>,
    afterfn: Arc<Mutex<AfterFn<T, S>>>,
    conn_id: AtomicU64,
    exit_tx: Option<Sender<NetError>>,
}
unsafe impl<T, E, S: ConnBaseTrait> Send for Server<T, E, S> {}
impl<T, E> Server<T, E, NoSSL>
where
    E: EventHandler<T, NoSSL> + Send + Copy + Sync + 'static,
    T: Send + 'static,
{
    pub fn new_with_nossl(addr: &str, eventhandler: E) -> Self {
        Server::new(addr, eventhandler)
    }
}
impl<T, E, S> Server<T, E, S>
where
    E: EventHandler<T, S> + Send + Copy + Sync + 'static,
    T: Send + 'static,
    S: ConnBaseTrait + 'static,
{
    /// Server::<HttpAgent, Handler,net::conn::NoSSL>::new(
    pub fn new(addr: &str, eventhandler: E) -> Server<T, E, S> {
        let conn_map: HashMap<SocketAddr, Sender<ReactOperationChannel>> = Default::default();
        let conn_map = Arc::new(Mutex::new(conn_map));
        let afterfn = Arc::new(Mutex::new(AfterFn::<T, S>::new()));
        Server {
            addr: addr.to_string(),
            max_package_size: MAX as usize,
            eventhandler,
            readtimeout: Duration::from_secs(60),
            writetimeout: Duration::from_secs(60),
            max_buf_size: buffer::MAX_BUF_SIZE,
            default_buf_size: buffer::DEFAULT_BUF_SIZE,
            buf_num: 128,
            bufpool: None,
            conn_map,
            afterfn,
            conn_id: AtomicU64::new(0),
            exit_tx: None,
        }
    }
    //pub fn with_eventhandler(eventhandler: E) {}
    pub fn with_max_package_size(mut self, size: usize) -> Self {
        self.max_package_size = size;
        self
    }
    pub fn with_readtimeout(mut self, readtimeout: Duration) -> Self {
        self.readtimeout = readtimeout;
        self
    }
    pub fn with_writetimeout(mut self, writetimeout: Duration) -> Self {
        self.writetimeout = writetimeout;
        self
    }

    //将会保留 size * max_buf_size
    pub fn with_max_buf_num(mut self, size: usize) -> Self {
        self.buf_num = size;

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
    pub async fn start(mut self) -> Result<(), NetError> {
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

        let listener = TcpListener::bind(&addr).await?;
        let mut intervalsec = time::interval(Duration::from_secs(1));
        let mut intervalms = time::interval(Duration::from_millis(1));
        let (exit_tx, mut exit_rx) = mpsc::channel::<NetError>(128);
        self.exit_tx = Some(exit_tx);
        let server = Arc::new(self);
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
                        let f1 = &mut exit_rx.recv();
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
                        let _ = server
                            .start_conn_handle(
                                server.clone(),
                                conn::Stream::Tcp(stream),
                                addr,
                                None,
                            )
                            .await;
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
                            if let Some(react_tx) = conn_map.lock().await.get_mut(&addr) {
                                react_tx.try_send(ReactOperationChannel {
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
}

//EventHandler后面接具体的ctx类型
#[async_trait]
pub trait EventHandler<T: Send, S: ConnBaseTrait> {
    /// with_param,可以传入一个初始化参数
    /// 返回一个ctx
    async fn on_opened(&mut self, conn: &mut ConnBase<T, S>) -> Result<T, NetError>;

    /// 返回true，则表示continue，处理下一个消息；false为不是一个完整消息
    /// 例如：收到一个web请求，但是消息未接收完整，返回false，等待下次接收更多消息重新处理
    async fn react(&mut self, conn: &mut ConnBase<T, S>, ctx: &mut T) -> Result<bool, NetError>;

    async fn on_closed(
        &mut self,
        _conn: &mut ConnBase<T, S>,
        _ctx: Option<T>,
        _reasion: Option<String>,
    ) -> Result<(), NetError> {
        Ok(())
    }
}

#[async_trait]
pub trait ServerStartConn<T, S: ConnBaseTrait>: Send + Sync {
    async fn start_conn_handle(
        &self,
        server: Arc<dyn ServerStartConn<T, S>>,
        stream: Stream<S>,
        addr: SocketAddr,
        callback: Option<Box<dyn FnOnce(T) -> Result<T, NetError> + Send>>,
    ) -> Result<ConnWrite, NetError>;
    fn get_exit_tx(&self)-> Option<Sender<NetError>>;
}

#[async_trait]
impl<T, E, S> ServerStartConn<T, S> for Server<T, E, S>
where
    E: EventHandler<T, S> + Send + Copy + Sync + 'static,
    T: Send + 'static,
    S: ConnBaseTrait + 'static,
{
    async fn start_conn_handle(
        &self,
        server: Arc<dyn ServerStartConn<T, S>>,
        stream: Stream<S>,
        addr: SocketAddr,
        callback: Option<Box<dyn FnOnce(T) -> Result<T, NetError> + Send>>,
    ) -> Result<ConnWrite, NetError> {
        let conn_map = self.conn_map.clone();
        let afterfn = self.afterfn.clone();
        let mut eventhandler = self.eventhandler;
        let max_package_size = self.max_package_size;
        let (react_tx, react_rx) = mpsc::channel::<ReactOperationChannel>(99);
        let connwrite = ConnWrite {
            addr:addr,
            react_tx: react_tx.clone(),
        };
        let connid = self
            .conn_id
            .fetch_add(1, std::sync::atomic::Ordering::Acquire);

        let wg = AsyncWaitGroup::new();
        let _react_tx = react_tx.clone();

        //启动react
        let reactwg = wg.add(1);
        let mut conn = Conn::<T, S>::new(
            addr,
            connid,
            react_tx.clone(),
            afterfn,
            self.readtimeout,
            stream,
            max_package_size,
            server.clone(),
        );
        {
            let mut map = conn_map.lock().await;
            map.insert(addr, react_tx);
        }
        tokio::spawn(async move {
            let mut ctx = match on_opened(&mut conn.inner, eventhandler, callback).await {
                Ok(ctx) => ctx,
                Err(e) => {
                    if let Err(_e) = eventhandler
                        .on_closed(&mut conn.inner, None, Some(e.to_string()))
                        .await
                    {
                        #[cfg(debug_assertions)]
                        println!("on_closed 退出于错误 {:?}", _e);
                    };
                    return;
                }
            };

            let exit_reason =
                match handler_reactreadwrite(&mut conn, &mut ctx, eventhandler, react_rx).await {
                    Ok(exit_reason) => exit_reason,
                    Err(e) => {
                        #[cfg(debug_assertions)]
                        println!("react 退出于错误 {:?}", e);

                        Some(e.to_string())
                    }
                };
            if let Err(_e) = eventhandler
                .on_closed(&mut conn.inner, Some(ctx), exit_reason)
                .await
            {
                #[cfg(debug_assertions)]
                println!("on_closed 退出于错误 {:?}", _e);
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
        return Ok(connwrite);
    }
    fn get_exit_tx(&self) -> Option<Sender<NetError>> {
        self.exit_tx.clone()
    }
}
async fn on_opened<T, E, S>(
    conn: &mut ConnBase<T, S>,
    mut event_handler: E,
    callback: Option<Box<dyn FnOnce(T) -> Result<T, NetError> + Send>>,
) -> Result<T, NetError>
where
    E: EventHandler<T, S> + Send + Copy + Sync + 'static,
    T: Send + 'static,
    S: ConnBaseTrait + 'static,
{
    let ctx = match callback {
        Some(f) => f.call_once((event_handler.on_opened(conn).await?,))?,
        None => event_handler.on_opened(conn).await?,
    };
    Ok(ctx)
}
pub struct NoServer {}
#[async_trait]
impl<T: 'static, S: ConnBaseTrait + 'static> ServerStartConn<T, S> for NoServer {
    async fn start_conn_handle(
        &self,
        _server: Arc<dyn ServerStartConn<T, S>>,
        _stream: Stream<S>,
        _addr: SocketAddr,
        _callback: Option<Box<dyn FnOnce(T) -> Result<T, NetError> + Send>>,
    ) -> Result<ConnWrite, NetError> {
        Err(NetError::NoStartServer)
    }
    fn get_exit_tx(&self) -> Option<Sender<NetError>> {
        None
    }
}
