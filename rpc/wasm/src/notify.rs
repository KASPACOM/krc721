use crate::imports::*;
use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEADER: &'static str = r#"

/**
 * RPC notification events.
 * 
 * @category KRC-721 RPC
 * 
 * @see {RpcClient.addEventListener}, {RpcClient.removeEventListener}
 */
export enum Krc721RpcEventType {
    Connect = "connect",
    Disconnect = "disconnect",
    TestNotification = "test-notification",
}

/**
 * RPC notification data payload.
 * 
 * @category KRC-721 RPC
 */
export type Krc721RpcEventData = ITestNotification;

/**
 * RPC notification event data map.
 * 
 * @category KRC-721 RPC
 */
export type Krc721RpcEventMap = {
    "connect" : undefined,
    "disconnect" : undefined,
    "test-notification" : ITestNotification,
}

/**
 * RPC notification event.
 * 
 * @category KRC-721 RPC
 */
export type Krc721RpcEvent = {
    [K in keyof Krc721RpcEventMap]: { event: K, data: Krc721RpcEventMap[K] }
}[keyof Krc721RpcEventMap];

/**
 * RPC notification callback type.
 * 
 * This type is used to define the callback function that is called when an RPC notification is received.
 * 
 * @see {@link RpcClient.subscribeVirtualDaaScoreChanged},
 * {@link RpcClient.subscribeTestNotification}, 
 * 
 * @category KRC-721 RPC
 */
export type Krc721RpcEventCallback = (event: Krc721RpcEvent) => void;

"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Function, typescript_type = "Krc721RpcEventCallback")]
    pub type Krc721RpcEventCallback;

    #[wasm_bindgen(extends = js_sys::Function, typescript_type = "Krc721RpcEventType | string")]
    #[derive(Debug)]
    pub type Krc721RpcEventType;

    #[wasm_bindgen(typescript_type = "Krc721RpcEventType | string | Krc721RpcEventCallback")]
    #[derive(Debug)]
    pub type Krc721RpcEventTypeOrCallback;
}

declare! {
    ITestNotification,
    r#"
    /**
     * Test Notification.
     * 
     * @category KRC-721 RPC
     */
    export interface ITestNotification {
        [key: string]: any;
    }
    "#,
}

