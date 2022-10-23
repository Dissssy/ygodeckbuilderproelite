#[macro_export]
macro_rules! block {
    ($x:expr) => {{
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        rt.block_on($x)
    }};
}

#[macro_export]
macro_rules! promisify {
    ($future:expr) => {{
        let (sender, promise) = poll_promise::Promise::new();

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move { sender.send($future.await) });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

            rt.block_on(async move { sender.send($future.await) });
        }

        promise
    }};
}
