const uniswapSpellABI = [
  {
    inputs: [
      {
        internalType: "contract IBank",
        name: "_bank",
        type: "address",
      },
      {
        internalType: "address",
        name: "_werc20",
        type: "address",
      },
      {
        internalType: "contract IUniswapV2Router02",
        name: "_router",
        type: "address",
      },
    ],
    stateMutability: "nonpayable",
    type: "constructor",
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "governor",
        type: "address",
      },
    ],
    name: "AcceptGovernor",
    type: "event",
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "governor",
        type: "address",
      },
    ],
    name: "SetGovernor",
    type: "event",
  },
  {
    anonymous: false,
    inputs: [
      {
        indexed: false,
        internalType: "address",
        name: "pendingGovernor",
        type: "address",
      },
    ],
    name: "SetPendingGovernor",
    type: "event",
  },
  {
    inputs: [],
    name: "acceptGovernor",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "tokenA",
        type: "address",
      },
      {
        internalType: "address",
        name: "tokenB",
        type: "address",
      },
      {
        components: [
          {
            internalType: "uint256",
            name: "amtAUser",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBUser",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPUser",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtABorrow",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBBorrow",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPBorrow",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtAMin",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBMin",
            type: "uint256",
          },
        ],
        internalType: "struct UniswapV2SpellV1.Amounts",
        name: "amt",
        type: "tuple",
      },
    ],
    name: "addLiquidityWERC20",
    outputs: [],
    stateMutability: "payable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "tokenA",
        type: "address",
      },
      {
        internalType: "address",
        name: "tokenB",
        type: "address",
      },
      {
        components: [
          {
            internalType: "uint256",
            name: "amtAUser",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBUser",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPUser",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtABorrow",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBBorrow",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPBorrow",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtAMin",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBMin",
            type: "uint256",
          },
        ],
        internalType: "struct UniswapV2SpellV1.Amounts",
        name: "amt",
        type: "tuple",
      },
      {
        internalType: "address",
        name: "wstaking",
        type: "address",
      },
    ],
    name: "addLiquidityWStakingRewards",
    outputs: [],
    stateMutability: "payable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    name: "approved",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [],
    name: "bank",
    outputs: [
      {
        internalType: "contract IBank",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [],
    name: "factory",
    outputs: [
      {
        internalType: "contract IUniswapV2Factory",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "tokenA",
        type: "address",
      },
      {
        internalType: "address",
        name: "tokenB",
        type: "address",
      },
    ],
    name: "getAndApprovePair",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [],
    name: "governor",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "wstaking",
        type: "address",
      },
    ],
    name: "harvestWStakingRewards",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
      {
        internalType: "address",
        name: "",
        type: "address",
      },
      {
        internalType: "uint256[]",
        name: "",
        type: "uint256[]",
      },
      {
        internalType: "uint256[]",
        name: "",
        type: "uint256[]",
      },
      {
        internalType: "bytes",
        name: "",
        type: "bytes",
      },
    ],
    name: "onERC1155BatchReceived",
    outputs: [
      {
        internalType: "bytes4",
        name: "",
        type: "bytes4",
      },
    ],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
      {
        internalType: "address",
        name: "",
        type: "address",
      },
      {
        internalType: "uint256",
        name: "",
        type: "uint256",
      },
      {
        internalType: "uint256",
        name: "",
        type: "uint256",
      },
      {
        internalType: "bytes",
        name: "",
        type: "bytes",
      },
    ],
    name: "onERC1155Received",
    outputs: [
      {
        internalType: "bytes4",
        name: "",
        type: "bytes4",
      },
    ],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    name: "pairs",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [],
    name: "pendingGovernor",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "tokenA",
        type: "address",
      },
      {
        internalType: "address",
        name: "tokenB",
        type: "address",
      },
      {
        components: [
          {
            internalType: "uint256",
            name: "amtLPTake",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPWithdraw",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtARepay",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBRepay",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPRepay",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtAMin",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBMin",
            type: "uint256",
          },
        ],
        internalType: "struct UniswapV2SpellV1.RepayAmounts",
        name: "amt",
        type: "tuple",
      },
    ],
    name: "removeLiquidityWERC20",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "tokenA",
        type: "address",
      },
      {
        internalType: "address",
        name: "tokenB",
        type: "address",
      },
      {
        components: [
          {
            internalType: "uint256",
            name: "amtLPTake",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPWithdraw",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtARepay",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBRepay",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtLPRepay",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtAMin",
            type: "uint256",
          },
          {
            internalType: "uint256",
            name: "amtBMin",
            type: "uint256",
          },
        ],
        internalType: "struct UniswapV2SpellV1.RepayAmounts",
        name: "amt",
        type: "tuple",
      },
      {
        internalType: "address",
        name: "wstaking",
        type: "address",
      },
    ],
    name: "removeLiquidityWStakingRewards",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [],
    name: "router",
    outputs: [
      {
        internalType: "contract IUniswapV2Router02",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "_pendingGovernor",
        type: "address",
      },
    ],
    name: "setPendingGovernor",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address[]",
        name: "lpTokens",
        type: "address[]",
      },
      {
        internalType: "bool[]",
        name: "statuses",
        type: "bool[]",
      },
    ],
    name: "setWhitelistLPTokens",
    outputs: [],
    stateMutability: "nonpayable",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "bytes4",
        name: "interfaceId",
        type: "bytes4",
      },
    ],
    name: "supportsInterface",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [],
    name: "werc20",
    outputs: [
      {
        internalType: "contract IWERC20",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [],
    name: "weth",
    outputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    inputs: [
      {
        internalType: "address",
        name: "",
        type: "address",
      },
    ],
    name: "whitelistedLpTokens",
    outputs: [
      {
        internalType: "bool",
        name: "",
        type: "bool",
      },
    ],
    stateMutability: "view",
    type: "function",
  },
  {
    stateMutability: "payable",
    type: "receive",
  },
];

module.exports = {uniswapSpellABI};