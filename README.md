# rustnet

rust基于tokio的net轻框架，api灵感来源于go的gnet v1，目前开发中

-------------

### 使用方式：定义一个EventHandler<T, S>，有三个接口

* on_opened(&mut self, conn)->Result<T>
* react(&mut self,conn,T)
* on_close(conn,T,reason)

#### on_opened
当一个客户端请求到服务器时候，会调用on_opened，在这里你可以对<T>(ctx)进行初始化,T的生命周期贯穿整个连接存活期，适合保存session或者像下面的例子保存代理的对方连接

#### react
当一个数据包到达的时候，会调用react，这时候你可以取出来buffer_data，进行判断消息是否完成，消息未完成返回false。然后通过buffer_shift跳过指定长度数据包或者buffer_reset清空，处理完一个消息后返回ture

#### on_closed
当发生error或者客户端主动断开连接，都会调用此接口，你可以在断开连接后做一些事情

-------------

### Server启动方式
Server::new_with_nossl(addr,EventHandler).start()

-------------
以下是架设一个简单端口代理的例子
```rust
use async_trait::async_trait;
use rustnet::err::NetError;
use rustnet::*;
use static_init::dynamic;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listen = "0.0.0.0:30020";
    let server =
        Server::new_with_nossl(listen, Handler {}).with_writetimeout(Duration::from_secs(10));
    println!("start proxy server {listen}");
    Ok(server.start().await?)
}
pub struct Agent {
    remote_agent: Option<ConnWrite>,
}
#[dynamic]
static TARGET_ADDR: SocketAddr = "172.30.0.3:30020".parse().unwrap();

#[derive(Copy, Clone)]
pub struct Handler {}
#[async_trait]
impl<S: ConnBaseTrait> EventHandler<Agent, S> for Handler {
    async fn on_opened(&mut self, conn: &mut ConnBase<Agent, S>) -> Result<Agent, NetError> {
        if TARGET_ADDR.to_owned() == conn.addr() {
            //remote_agent,与服务器连接,
            return Ok(Agent { remote_agent: None });
        }
        #[cfg(debug_assertions)]
        println!("進來了{:?}", conn.addr());
        //连接到远程代理服务器
        let socket = TcpSocket::new_v4().unwrap();
        let stream = socket.connect(TARGET_ADDR.to_owned()).await?;
        let local_w=conn.get_write_conn()?;
        let w = conn
            .server
            .start_conn_handle(
                conn.server.clone(),
                Stream::Tcp(stream),
                TARGET_ADDR.to_owned(),
                Some(Box::new(|mut ctx: Agent| {
                    //给代理conn增加本conn
                    ctx.remote_agent = Some(local_w);
                    Ok(ctx)
                })),
            )
            .await?;
        Ok(Agent {
            remote_agent: Some(w),
        })
    }
    async fn react(
        &mut self,
        conn: &mut ConnBase<Agent, S>,
        ctx: &mut Agent,
    ) -> Result<bool, NetError> {
        //获取消息
        let data = conn.buffer_data();

        #[cfg(debug_assertions)]
        println!("收到消息{:?}", data);
        //往对方发消息
        ctx.remote_agent
            .as_ref()
            .unwrap()
            .write(data.to_vec())
            .await?;
        //清空缓存
        conn.buffer_reset();
        Ok(true)
    }
}

```
这段代码是监听0.0.0.0:30020,并将此端口的数据代理到172.30.0.3:30020

-------------
引用方式
```rust
[dependencies]
rustnet = { git = "https://github.com/luyu6056/rustnet.git", version = "0.1.1"}
```

-------------

## 将要做的事情 Features
- [ ] **TLS/SSL** support
- [ ] **Client** support

