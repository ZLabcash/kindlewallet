//! The EVM networks behind the one Ethereum address (m/44'/60'/0'/0/i): the same
//! 0x… receives on every chain below, only the chain id in a signed transaction
//! tells them apart.

/// One EVM chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvmNetwork {
    pub name: &'static str,
    /// Its native gas token.
    pub symbol: &'static str,
    /// EIP-155 chain id (as listed on chainid.network).
    pub chain_id: u64,
    /// The logo's file stem in assets/logos (see `logos::evm_logo`).
    pub key: &'static str,
}

const fn net(name: &'static str, symbol: &'static str, chain_id: u64, key: &'static str) -> EvmNetwork {
    EvmNetwork { name, symbol, chain_id, key }
}

/// By total value locked (DefiLlama, October 2026), largest first.
pub const NETWORKS: &[EvmNetwork] = &[
    net("Ethereum", "ETH", 1, "ethereum"),
    net("Base", "ETH", 8453, "base"),
    net("BNB Chain", "BNB", 56, "binance"),
    net("Arbitrum", "ETH", 42161, "arbitrum"),
    // 999 is also listed for an old Wanchain testnet; Hyperliquid's RPC answers 0x3e7.
    net("HyperEVM", "HYPE", 999, "hyperevm"),
    net("Robinhood Chain", "ETH", 4663, "robinhood"),
    net("Monad", "MON", 143, "monad"),
    net("Polygon", "POL", 137, "polygon"),
    net("Avalanche C-Chain", "AVAX", 43114, "avalanche"),
    net("Plasma", "XPL", 9745, "plasma"),
    net("OP Mainnet", "ETH", 10, "optimism"),
    net("Ink", "ETH", 57073, "ink"),
    net("Cronos", "CRO", 25, "cronos"),
    net("Mantle", "MNT", 5000, "mantle"),
    net("Gnosis", "XDAI", 100, "gnosis"),
    net("Linea", "ETH", 59144, "linea"),
    net("Sonic", "S", 146, "sonic"),
    net("Scroll", "ETH", 534352, "scroll"),
    net("World Chain", "ETH", 480, "worldchain"),
    net("Unichain", "ETH", 130, "unichain"),
    net("Berachain", "BERA", 80094, "berachain"),
    net("zkSync Era", "ETH", 324, "zksync"),
    net("Celo", "CELO", 42220, "celo"),
    net("Sei EVM", "SEI", 1329, "sei"),
];

/// The network with this chain id.
pub fn by_chain_id(chain_id: u64) -> Option<&'static EvmNetwork> {
    NETWORKS.iter().find(|n| n.chain_id == chain_id)
}
