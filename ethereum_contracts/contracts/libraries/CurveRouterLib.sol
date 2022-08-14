//SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/ICurve.sol";

// Information on a single swap with a Curve pool.
struct CurveSwapOperation {
    // Curve pool address.
    address pool;
    // Index of the token in the pool to be swapped.
    int128 from_index;
    // Index of the token in the pool to be returned.
    int128 to_index;
    // If true, use exchange_underlying(); otherwise, use exchange().
    bool underlying;
}

struct CurveRouterContext {
    // The array `routes[from_token][to_token]` stores Curve swap operations that achieve the exchange from `from_token` to `to_token`.
    mapping(address => mapping(address => CurveSwapOperation[])) routes;
}

library CurveRouterLib {
    using SafeERC20 for IERC20;

    // Updates the Curve swap route for `fromToken` to `toToken` with `route`.
    // The array `tokens` should comprise all tokens on `route` except for `toToken`.
    // Each element of `tokens` needs to be swapped for another token through some Curve pool, so we need to allow the pool to transfer the corresponding token from this contract.
    //
    // Examples:
    // (1) BUSD -> whUST route: [[CURVE_BUSD_3CRV_POOL_ADDR, 0, 1, false], [CURVE_WHUST_3CRV_POOL_ADDR, 1, 0, false]];
    //     tokens: [BUSD_TOKEN_ADDR, 3CRV_TOKEN_ADDR];
    //     The first exchange: BUSD -> 3Crv using the BUSD-3Crv pool;
    //     The second exchange: 3Crv -> whUST using the whUST-3Crv pool.
    // (2) USDC -> whUST route: [[CURVE_WHUST_3CRV_POOL_ADDR, 2, 0, true]];
    //     tokens: [USDC_TOKEN_ADDR];
    //     The only underlying exchange: USDC -> whUST using the whUST-3Crv pool's exchange_underlying() function.
    function updateRoute(
        CurveRouterContext storage self,
        address fromToken,
        address toToken,
        CurveSwapOperation[] calldata route,
        address[] calldata tokens
    ) external {
        require(route.length > 0 && route.length == tokens.length);
        for (uint256 i = 0; i < route.length; i++) {
            if (
                IERC20(tokens[i]).allowance(address(this), route[i].pool) == 0
            ) {
                IERC20(tokens[i]).safeIncreaseAllowance(
                    route[i].pool,
                    type(uint256).max
                );
            }
        }
        CurveSwapOperation[] storage storage_route = self.routes[fromToken][
            toToken
        ];
        if (storage_route.length != 0) {
            delete self.routes[fromToken][toToken];
        }
        for (uint256 i = 0; i < route.length; ++i) {
            storage_route.push(route[i]);
        }
    }

    // Swaps `fromToken` in the amount of `amount` to `toToken`.
    // Revert if the output amount is less `minAmountOut`.
    // Returns the output amount.
    //
    // Note that `curveSwapRoutes` also acts as a whitelist on `fromToken`.
    // That is to say, if a swap route is not set for `fromToken` -> `toToken`, then this function reverts
    // without calling ` IERC20(fromToken).safeTransferFrom()`.
    // This prevents re-entrancy attacks due to malicious `fromToken` contracts.
    function swapToken(
        CurveRouterContext storage self,
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut
    ) external returns (uint256) {
        CurveSwapOperation[] storage route = self.routes[fromToken][toToken];
        require(route.length > 0, "Swap route does not exist");

        for (uint256 i = 0; i < route.length; i++) {
            if (route[i].underlying) {
                amount = ICurve(route[i].pool).exchange_underlying(
                    route[i].from_index,
                    route[i].to_index,
                    amount,
                    0
                );
            } else {
                amount = ICurve(route[i].pool).exchange(
                    route[i].from_index,
                    route[i].to_index,
                    amount,
                    0
                );
            }
        }

        require(
            amount >= minAmountOut,
            "Output token amount less than specified minimum"
        );
        return amount;
    }

    // Simulates the swap from `amount` amount of `fromToken` to `toToken` and returns the output amount.
    // Note that this function chains together simulations of Curve pool exchanges; assumes that each Curve pool exchange does not have any side effects on subsequent exchanges.
    function simulateSwapToken(
        CurveRouterContext storage self,
        address fromToken,
        address toToken,
        uint256 amount
    ) external view returns (uint256) {
        CurveSwapOperation[] storage route = self.routes[fromToken][toToken];
        require(route.length > 0, "Swap route does not exist");
        for (uint256 i = 0; i < route.length; i++) {
            if (route[i].underlying) {
                amount = ICurve(route[i].pool).get_dy_underlying(
                    route[i].from_index,
                    route[i].to_index,
                    amount
                );
            } else {
                amount = ICurve(route[i].pool).get_dy(
                    route[i].from_index,
                    route[i].to_index,
                    amount
                );
            }
        }
        return amount;
    }
}
