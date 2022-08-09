// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

interface IHomoraOracle {
    function tokenFactors(address token)
    external
    view
    returns (
        uint16 borrowFactor,
        uint16 collateralFactor,
        uint16 liqIncentive
    );

    /// @dev Return whether the ERC20 token is supported
    /// @param token The ERC20 token to check for support
    function support(address token) external view override returns (bool);

    /// @dev Return whether the oracle supports evaluating collateral value of the given token.
    /// @param token ERC1155 token address to check for support
    /// @param id ERC1155 token id to check for support
    function supportWrappedToken(address token, uint id) external view override returns (bool);

    /// @dev Return the value of the given input as ETH for collateral purpose.
    /// @param token ERC1155 token address to get collateral value
    /// @param id ERC1155 token id to get collateral value
    /// @param amount Token amount to get collateral value
    /// @param owner Token owner address (currently unused by this implementation)
    function asETHCollateral(
        address token,
        uint id,
        uint amount,
        address owner
    ) external view override returns (uint);

    /// @dev Return the value of the given input as ETH for borrow purpose.
    /// @param token ERC20 token address to get borrow value
    /// @param amount ERC20 token amount to get borrow value
    /// @param owner Token owner address (currently unused by this implementation)
    function asETHBorrow(
        address token,
        uint amount,
        address owner
    ) external view override returns (uint);
}
