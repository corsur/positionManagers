// SPDX-License-Identifier: BUSL-1.1
pragma solidity >=0.8.0 <0.9.0;

import "../interfaces/IHomoraOracle.sol";

library OracleLib {
    function support(address oracle, address token)
        public view
        returns (bool)
    {
        return IHomoraOracle(oracle).support(token);
    }

    function supportLP(address oracle, address lpToken)
        public view
        returns (bool)
    {
        (, , uint16 liqIncentive) = IHomoraOracle(oracle).tokenFactors(lpToken);
        return liqIncentive != 0;
    }

    /// @dev Query the collateral factor of the LP token on Homora, 0.84 => 8400
    function getCollateralFactor(address oracle, address lpToken)
        public view
        returns (uint256 _collateralFactor)
    {
        (, _collateralFactor, ) = IHomoraOracle(oracle).tokenFactors(lpToken);
        require(0 < _collateralFactor && _collateralFactor < 10000, "Invalid collateral factor");
    }

    /// @dev Query the borrow factor of the debt token on Homora, 1.04 => 10400
    /// @param token: Address of the ERC-20 debt token
    function getBorrowFactor(address oracle, address token)
        public view
        returns (uint256 _borrowFactor)
    {
        (_borrowFactor, , ) = IHomoraOracle(oracle).tokenFactors(token);
        require(_borrowFactor > 10000, "Invalid borrow factor");
    }

    /// @dev Return the value of the given token as ETH, weighted by the borrow factor
    function asETHBorrow(
        address oracle,
        address token,
        uint256 amount,
        address owner
    )
        public view
        returns (uint256)
    {
        return IHomoraOracle(oracle).asETHBorrow(token, amount, owner);
    }
}
