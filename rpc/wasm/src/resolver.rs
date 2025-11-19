use crate::client::{RpcClient, RpcConfig};
use crate::imports::*;
use js_sys::Array;
pub use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
use kaspa_wrpc_client::node::NodeDescriptor;
use kaspa_wrpc_client::Resolver as NativeKrc721Resolver;
use serde::ser;
use workflow_wasm::extensions::ObjectExtension;

declare! {
    IResolverConfig,
    "IKrc721ResolverConfig | string[]",
    r#"
    /**
     * RPC Resolver configuration options
     * 
     * @category KRC-721 RPC
     */
    export interface IKrc721ResolverConfig {
        /**
         * Optional URLs for one or multiple resolvers.
         */
        urls?: string[];
    }
    "#,
}

declare! {
    IKrc721ResolverConnect,
    "IKrc721ResolverConnect | NetworkId | string",
    r#"
    /**
     * RPC Resolver connection options
     * 
     * @category KRC-721 RPC
     */
    export interface IKrc721ResolverConnect {
        /**
         * RPC encoding: `borsh` (default) or `json`
         */
        encoding?: Encoding | string;
        /**
         * Network identifier: `mainnet` or `testnet-10` etc.
         */
        networkId?: NetworkId | string;
    }
    "#,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverConnect {
    pub encoding: Option<Encoding>,
    pub network_id: NetworkId,
}

impl TryFrom<IResolverConnect> for ResolverConnect {
    type Error = Error;
    fn try_from(config: IResolverConnect) -> Result<Self> {
        if let Ok(network_id) = NetworkId::try_owned_from(&config) {
            Ok(Self { encoding: None, network_id })
        } else {
            Ok(serde_wasm_bindgen::from_value(config.into())?)
        }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "string[]")]
    pub type Krc721ResolverArrayT;
}


#[derive(Debug, Clone, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct Krc721Resolver {
    resolver: NativeKrc721Resolver,
}

impl Krc721Resolver {
    pub fn new(resolver: NativeKrc721Resolver) -> Self {
        Self { resolver }
    }
}

#[wasm_bindgen]
impl Krc721Resolver {
    /// Creates a new Resolver client with the given
    /// configuration supplied as {@link IResolverConfig}
    /// interface. If not supplied, the default configuration
    /// containing a list of community-operated resolvers
    /// will be used.
    #[wasm_bindgen(constructor)]
    pub fn ctor(args: Option<IKrc721ResolverConfig>) -> Result<Krc721Resolver> {
        if let Some(args) = args {
            Ok(Self { resolver: NativeKrc721Resolver::try_from(args)? })
        } else {
            Ok(Self { resolver: NativeKrc721Resolver::default() })
        }
    }
}

#[wasm_bindgen]
impl Resolver {
    /// List of public Kaspa Resolver URLs.
    #[wasm_bindgen(getter)]
    pub fn urls(&self) -> Krc721ResolverArrayT {
        Array::from_iter(self.resolver.urls().iter().map(|v| JsValue::from(v.as_str()))).unchecked_into()
    }

    /// Fetches a public Kaspa wRPC endpoint for the given encoding and network identifier.
    /// @see {@link Encoding}, {@link NetworkId}, {@link Node}
    #[wasm_bindgen(js_name = getNode)]
    pub async fn get_node(&self, encoding: Encoding, network_id: NetworkIdT) -> Result<NodeDescriptor> {
        self.resolver.get_node(encoding, *network_id.try_into_cast()?).await
    }

    /// Fetches a public Kaspa wRPC endpoint URL for the given encoding and network identifier.
    /// @see {@link Encoding}, {@link NetworkId}
    #[wasm_bindgen(js_name = getUrl)]
    pub async fn get_url(&self, encoding: Encoding, network_id: NetworkIdT) -> Result<String> {
        self.resolver.get_url(encoding, *network_id.try_into_cast()?).await
    }

    /// Connect to a public Kaspa wRPC endpoint for the given encoding and network identifier
    /// supplied via {@link IResolverConnect} interface.
    /// @see {@link IResolverConnect}, {@link RpcClient}
    pub async fn connect(&self, options: IKrc721ResolverConnect) -> Result<RpcClient> {
        let ResolverConnect { encoding, network_id } = options.try_into()?;
        let config = RpcConfig { resolver: Some(self.clone()), url: None, encoding, network_id: Some(network_id) };
        let client = RpcClient::new(Some(config))?;
        client.connect(None).await?;
        Ok(client)
    }
}

impl TryFrom<IKrc721ResolverConfig> for NativeKrc721Resolver {
    type Error = Error;
    fn try_from(config: IResolverConfig) -> Result<Self> {
        let resolver = config
            .get_vec("urls")
            .map(|urls| urls.into_iter().map(|v| v.as_string()).collect::<Option<Vec<_>>>())
            .or_else(|_| config.dyn_into::<Array>().map(|urls| urls.into_iter().map(|v| v.as_string()).collect::<Option<Vec<_>>>()))
            .map_err(|_| Error::custom("Invalid or missing resolver URL"))?
            .map(|urls| NativeKrc721Resolver::new(urls.into_iter().map(Arc::new).collect()));

        Ok(resolver.unwrap_or_default())
    }
}

impl TryCastFromJs for Krc721Resolver {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>> {
        Ok(Self::try_ref_from_js_value_as_cast(value)?)
    }
}

impl TryFrom<&JsValue> for Krc721Resolver {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> Result<Self> {
        Ok(Resolver::try_ref_from_js_value(js_value)?.clone())
    }
}

impl TryFrom<JsValue> for Krc721Resolver {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self> {
        Resolver::try_from(js_value.as_ref())
    }
}

impl From<Krc721Resolver> for NativeKrc721Resolver {
    fn from(resolver: Resolver) -> Self {
        resolver.resolver
    }
}

impl From<NativeKrc721Resolver> for Krc721Resolver {
    fn from(resolver: NativeKrc721Resolver) -> Self {
        Self { resolver }
    }
}
