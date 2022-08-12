//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/IHomoraAvaxRouter.sol";
import "./interfaces/IHomoraBank.sol";
import "./interfaces/IHomoraOracle.sol";
import "./interfaces/IHomoraSpell.sol";
import "./libraries/VaultLib.sol";
import "./libraries/OracleLib.sol";

// Allow external linking of library. Our library doesn't contain assembly and
// can't corrupt contract state to make it unsafe to upgrade.
/// @custom:oz-upgrades-unsafe-allow external-library-linking
contract HomoraPDNVault is
    Initializable,
    UUPSUpgradeable,
    OwnableUpgradeable,
    ReentrancyGuardUpgradeable,
    IStrategyManager
{
    using SafeERC20 for IERC20;

    struct Position {
        uint256 shareAmount;
    }

    // --- modifiers ---
    modifier onlyApertureManager() {
        require(msg.sender == apertureManager, "unauthorized position mgr op");
        _;
    }

    modifier onlyController() {
        require(isController[msg.sender], "unauthorized controller");
        _;
    }

    // --- constants ---
    uint256 private constant _NO_ID = 0;
    bytes private constant HARVEST_DATA =
        abi.encodeWithSignature("harvestWMasterChef()");
    bytes4 private constant ADD_LIQUIDITY_SIG =
        bytes4(
            keccak256(
                "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
            )
        );
    bytes4 private constant REMOVE_LIQUIDITY_SIG =
        bytes4(
            keccak256(
                "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
            )
        );

    // --- accounts ---
    address public apertureManager;
    address public feeCollector;
    mapping(address => bool) public isController;

    // --- config ---
    address public stableToken; // token 0
    address public assetToken; // token 1
    address public spell;
    address public rewardToken;
    address public lpToken; // ERC-20 LP token address
    uint256 public leverageLevel; // target leverage
    uint256 public pid; // pool id
    IHomoraBank public homoraBank;
    address public oracle; // HomoraBank's oracle for determining prices.
    IHomoraAvaxRouter public router;

    uint256 public collateralFactor; // LP collateral factor on Homora
    uint256 public stableBorrowFactor; // stable token borrow factor on Homora
    uint256 public assetBorrowFactor; // asset token borrow factor on Homora
    uint256 public targetDebtRatio; // target debt ratio * 10000, 92% -> 9200
    uint256 public minDebtRatio; // minimum debt ratio * 10000
    uint256 public maxDebtRatio; // maximum debt ratio * 10000
    uint256 public deltaThreshold; // delta deviation percentage * 10000

    uint256 public maxCapacity; // Maximum amount allowed in stable across the vault
    uint256 public maxOpenPerTx; // Maximum amount allowed in stable to add in one transaction
    uint256 public maxWithdrawPerTx; // Maximum amount allowed in stable to withdraw in one transaction
    uint256 lastCollectionTimestamp; // Last timestamp when collecting management fee

    ApertureFeeConfig public feeConfig;

    // --- state ---
    // positions[chainId][positionId] stores share information about the position identified by (chainId, positionId).
    mapping(uint16 => mapping(uint128 => Position)) public positions;
    // Position id of the PDN vault in HomoraBank. Zero for new position.
    uint256 public homoraBankPosId;
    uint256 public totalShareAmount;

    // --- event ---
    event LogDeposit(
        uint16 indexed chainId,
        uint128 indexed positionId,
        uint256 equityAmount,
        uint256 shareAmount
    );
    event LogWithdraw(
        address indexed _to,
        uint256 withdrawShareAmount,
        uint256 stableTokenWithdrawAmount,
        uint256 assetTokenWithdrawAmount,
        uint256 avaxWithdrawAmount
    );
    event LogRebalance(uint256 equityBefore, uint256 equityAfter);
    event LogReinvest(uint256 equityBefore, uint256 equityAfter);

    // --- error ---
    error HomoraPDNVault_PositionIsHealthy();
    error HomoraPDNVault_DeltaNotNeutral();
    error HomoraPDNVault_DebtRatioNotHealthy();
    error Vault_Limit_Exceeded();
    error Insufficient_Liquidity_Mint();
    error Zero_Withdrawal_Amount();
    error Insufficient_Withdrawn_Share();
    error Insufficient_Token_Withdrawn();
    error Slippage_Too_Large();

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        address _apertureManager,
        address _feeCollector,
        address _controller,
        address _stableToken,
        address _assetToken,
        address _homoraBank,
        address _spell,
        address _rewardToken,
        uint256 _pid
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();

        apertureManager = _apertureManager;
        feeCollector = _feeCollector;
        isController[_controller] = true;
        stableToken = _stableToken;
        assetToken = _assetToken;
        homoraBank = IHomoraBank(_homoraBank);
        oracle = homoraBank.oracle();
        require(OracleLib.support(oracle, _stableToken), "Oracle doesn't support stable token.");
        require(OracleLib.support(oracle, _assetToken), "Oracle doesn't support asset token.");

        spell = _spell;
        rewardToken = _rewardToken;
        pid = _pid;
        homoraBankPosId = _NO_ID;
        lpToken = IHomoraSpell(spell).pairs(stableToken, assetToken);
        require(lpToken != address(0), "Pair does not match the spell.");
        require(OracleLib.supportLP(oracle, lpToken), "Oracle doesn't support lpToken.");
        router = IHomoraAvaxRouter(IHomoraSpell(spell).router());
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    /// @dev Set config for delta neutral valut.
    /// @param _leverageLevel: Target leverage
    /// @param _targetDebtRatio: Target debt ratio * 10000
    /// @param _debtRatioWidth: Deviation of debt ratio * 10000
    /// @param _deltaThreshold: Delta deviation threshold in percentage * 10000
    /// @param _harvestFee: Fee collected on farming rewards
    /// @param _withdrawFee: Fee collected on user withdrawals
    /// @param _managementFee: Management fee, initially zero
    function initializeConfig(
        uint256 _leverageLevel,
        uint256 _targetDebtRatio,
        uint256 _debtRatioWidth,
        uint256 _deltaThreshold,
        uint256 _harvestFee,
        uint256 _withdrawFee,
        uint256 _managementFee
    ) external onlyOwner {
        setConfig(
            _leverageLevel,
            _targetDebtRatio,
            _debtRatioWidth,
            _deltaThreshold
        );
        setHarvestFee(_harvestFee);
        setWithdrawFee(_withdrawFee);
        setManagementFee(_managementFee);
    }

    function setControllers(
        address[] calldata controllers,
        bool[] calldata statuses
    ) public onlyOwner {
        require(
            controllers.length == statuses.length,
            "controllers & statuses length mismatched"
        );
        for (uint i = 0; i < controllers.length; i++) {
            isController[controllers[i]] = statuses[i];
        }
    }

    function setHarvestFee(uint256 fee) public onlyOwner {
        feeConfig.harvestFee = fee;
    }

    function setWithdrawFee(uint256 fee) public onlyOwner {
        feeConfig.withdrawFee = fee;
    }

    function setManagementFee(uint256 fee) public onlyOwner {
        feeConfig.managementFee = fee;
    }

    function setVaultLimits(
        uint256 _maxCapacity,
        uint256 _maxOpenPerTx,
        uint256 _maxWithdrawPerTx
    ) public onlyOwner {
        maxCapacity = _maxCapacity;
        maxOpenPerTx = _maxOpenPerTx;
        maxWithdrawPerTx = _maxWithdrawPerTx;
    }

    /// @dev Set config for delta neutral valut.
    /// @param _leverageLevel: Target leverage
    /// @param _targetDebtRatio: Target debt ratio * 10000
    /// @param _debtRatioWidth: Deviation of debt ratio * 10000
    /// @param _deltaThreshold: Delta deviation threshold in percentage * 10000
    function setConfig(
        uint256 _leverageLevel,
        uint256 _targetDebtRatio,
        uint256 _debtRatioWidth,
        uint256 _deltaThreshold
    ) public onlyOwner {
        require(_leverageLevel >= 2, "Leverage at least 2");
        leverageLevel = _leverageLevel;
        collateralFactor = OracleLib.getCollateralFactor(oracle, lpToken);
        stableBorrowFactor = OracleLib.getBorrowFactor(oracle, stableToken);
        assetBorrowFactor = OracleLib.getBorrowFactor(oracle, assetToken);
        uint256 calculatedDebtRatio = (stableBorrowFactor * (leverageLevel - 2) / leverageLevel
            + assetBorrowFactor) * 10000 / (2 * collateralFactor);
        require((_targetDebtRatio > calculatedDebtRatio
            ? _targetDebtRatio - calculatedDebtRatio
            : calculatedDebtRatio - _targetDebtRatio)
            <= 10, "Invalid debt ratio");

        targetDebtRatio = _targetDebtRatio;
        minDebtRatio = targetDebtRatio - _debtRatioWidth;
        maxDebtRatio = targetDebtRatio + _debtRatioWidth;
        require(0 < minDebtRatio && maxDebtRatio < 10000, "Invalid debt ratio");

        uint256 calculatedDeltaTh = leverageLevel * leverageLevel * collateralFactor * _debtRatioWidth
            / (leverageLevel * assetBorrowFactor - (leverageLevel - 2) * stableBorrowFactor);
        console.log("collateralFactor", collateralFactor);
        console.log("stableBorrowFactor", stableBorrowFactor);
        console.log("assetBorrowFactor", assetBorrowFactor);
        console.log("calculatedDebtRatio", calculatedDebtRatio);
        console.log("calculatedDeltaTh", calculatedDeltaTh);
        require((_deltaThreshold > calculatedDeltaTh
            ? _deltaThreshold - calculatedDeltaTh
            : calculatedDeltaTh - _deltaThreshold)
            <= 10, "Invalid delta threshold");
        deltaThreshold = _deltaThreshold;
    }

    receive() external payable {}

    /// @dev Open a new Aperture position for `recipient`
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity, etc
    function openPosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (
            uint256 stableTokenDepositAmount,
            uint256 assetTokenDepositAmount,
            uint256 minEquityETH,
            uint256 minReinvestETH
        ) = abi.decode(data, (uint256, uint256, uint256, uint256));
        depositInternal(
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            minEquityETH,
            minReinvestETH
        );
    }

    /// @dev Increase an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity, etc
    function increasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (
            uint256 stableTokenDepositAmount,
            uint256 assetTokenDepositAmount,
            uint256 minEquityETH,
            uint256 minReinvestETH
        ) = abi.decode(data, (uint256, uint256, uint256, uint256));
        depositInternal(
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            minEquityETH,
            minReinvestETH
        );
    }

    /// @dev Decrease an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: The recipient, the amount of shares to withdraw and the minimum amount of assets returned, etc
    function decreasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (
            address recipient,
            uint256 shareAmount,
            uint256 amtAMin,
            uint256 amtBMin,
            uint256 minReinvestETH
        ) = abi.decode(data, (address, uint256, uint256, uint256, uint256));
        withdrawInternal(
            position_info,
            recipient,
            shareAmount,
            amtAMin,
            amtBMin,
            minReinvestETH
        );
    }

    /// @dev Close an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Owner of the position on Aperture and the minimum amount of assets returned, etc
    function closePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (
            address recipient,
            uint256 amtAMin,
            uint256 amtBMin,
            uint256 minReinvestETH
        ) = abi.decode(data, (address, uint256, uint256, uint256));
        withdrawInternal(
            position_info,
            recipient,
            getShareAmount(position_info),
            amtAMin,
            amtBMin,
            minReinvestETH
        );
    }

    /// @dev Calculate the params passed to Homora to create PDN position
    /// @param stableTokenDepositAmount: The amount of stable token supplied by user
    /// @param assetTokenDepositAmount: The amount of asset token supplied by user
    function deltaNeutral(
        uint256 stableTokenDepositAmount,
        uint256 assetTokenDepositAmount
    )
        internal
        returns (
            uint256 stableTokenAmount,
            uint256 assetTokenAmount,
            uint256 stableTokenBorrowAmount,
            uint256 assetTokenBorrowAmount
        )
    {
        stableTokenAmount = stableTokenDepositAmount;

        // swap all assetTokens into stableTokens
        if (assetTokenDepositAmount > 0) {
            uint256 amount = swap(
                assetTokenDepositAmount,
                assetToken,
                stableToken
            );
            // update the stableToken amount
            stableTokenAmount += amount;
        }
        assetTokenAmount = 0;

        // total stableToken leveraged amount
        (uint256 reserve0, uint256 reserve1) = getReserves();
        uint256 totalAmount = stableTokenAmount * leverageLevel;
        uint256 desiredAmount = totalAmount / 2;
        stableTokenBorrowAmount = desiredAmount - stableTokenAmount;
        assetTokenBorrowAmount = router.quote(
            desiredAmount,
            reserve0,
            reserve1
        );
    }

    /// @dev Internal deposit function
    /// @param position_info: Aperture position info
    /// @param stableTokenDepositAmount: Amount of stable token supplied by user
    /// @param assetTokenDepositAmount: Amount of asset token supplied by user
    /// @param minEquityETH: Minimum equity received after adding liquidity
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function depositInternal(
        PositionInfo memory position_info,
        uint256 stableTokenDepositAmount,
        uint256 assetTokenDepositAmount,
        uint256 minEquityETH,
        uint256 minReinvestETH
    ) internal {
        // Check if the PDN position need rebalance
        if (!isDeltaNeutral() || !isDebtRatioHealthy()) {
            this.rebalance(10, minReinvestETH);
        } else {
            reinvestInternal(minReinvestETH);
        }

        // Record original position equity before adding liquidity
        uint256 equityBefore = getEquityETHValue();

        // Transfer user's deposit.
        if (stableTokenDepositAmount > 0)
            IERC20(stableToken).safeTransferFrom(
                msg.sender,
                address(this),
                stableTokenDepositAmount
            );
        if (assetTokenDepositAmount > 0)
            IERC20(assetToken).safeTransferFrom(
                msg.sender,
                address(this),
                assetTokenDepositAmount
            );

        uint256 stableTokenBorrowAmount;
        uint256 assetTokenBorrowAmount;
        (
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            stableTokenBorrowAmount,
            assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenDepositAmount, assetTokenDepositAmount);

//        (uint256 reserve0, uint256 reserve1) = _getReserves();
//        (
//            uint256 _stableTokenBorrowAmount,
//            uint256 _assetTokenBorrowAmount
//        ) = VaultLib.deltaNeutral(
//            stableTokenDepositAmount,
//            assetTokenDepositAmount,
//            reserve0,
//            reserve1,
//            leverageLevel
//        );

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(
            address(homoraBank),
            stableTokenDepositAmount
        );
        IERC20(assetToken).approve(
            address(homoraBank),
            assetTokenDepositAmount
        );

        bytes memory data = abi.encodeWithSelector(
            ADD_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [
                stableTokenDepositAmount,
                assetTokenDepositAmount,
                0,
                stableTokenBorrowAmount,
                assetTokenBorrowAmount,
                0,
                0,
                0
            ],
            pid
        );

        homoraBankPosId = homoraBank.execute(homoraBankPosId, spell, data);

        // Cancel HomoraBank's allowance.
        IERC20(stableToken).approve(address(homoraBank), 0);
        IERC20(assetToken).approve(address(homoraBank), 0);

        // Position equity after adding liquidity
        uint256 equityAfter = getEquityETHValue();
        uint256 equityChange = equityAfter - equityBefore;
        // Calculate user share amount.
        uint256 shareAmount = equityBefore == 0
            ? equityChange
            : (equityChange * totalShareAmount) / equityBefore;

        if (equityChange < minEquityETH) {
            revert Insufficient_Liquidity_Mint();
        }

        if (equityChange > OracleLib.getTokenETHValue(oracle, stableToken, maxOpenPerTx, msg.sender)) {
            revert Vault_Limit_Exceeded();
        }

        if (equityAfter > OracleLib.getTokenETHValue(oracle, stableToken, maxCapacity, msg.sender)) {
            revert Vault_Limit_Exceeded();
        }

        // Update vault position state.
        totalShareAmount += shareAmount;

        // Update deposit owner's position state.
        positions[position_info.chainId][position_info.positionId]
            .shareAmount += shareAmount;

        // Check if the PDN position is still healthy
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        emit LogDeposit(
            position_info.chainId,
            position_info.positionId,
            equityChange,
            shareAmount
        );
    }

    /// @dev Internal withdraw function
    /// @param position_info: Aperture position info
    /// @param withdrawShareAmount: Amount of user shares to withdraw
    /// @param minStableReceived: Minimum amount of stable token returned to user
    /// @param minAssetReceived: Minimum amount of asset token returned to user
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function withdrawInternal(
        PositionInfo memory position_info,
        address recipient,
        uint256 withdrawShareAmount,
        uint256 minStableReceived,
        uint256 minAssetReceived,
        uint256 minReinvestETH
    ) internal {
        if (withdrawShareAmount == 0) {
            revert Zero_Withdrawal_Amount();
        }
        if (withdrawShareAmount > getShareAmount(position_info)) {
            revert Insufficient_Withdrawn_Share();
        }
        // Check if the PDN position need rebalance
        if (!isDeltaNeutral() || !isDebtRatioHealthy()) {
            this.rebalance(10, minReinvestETH);
        } else {
            reinvestInternal(minReinvestETH);
        }

        collectManagementFee();

        // Record original position equity before removing liquidity
        uint256 equityBefore = getEquityETHValue();

        // Calculate collSize to withdraw.
        uint256 collWithdrawSize = (withdrawShareAmount * getCollateralSize()) /
        totalShareAmount;

        // Calculate debt to repay in two tokens.
        (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        ) = currentDebtAmount();

        // Encode removeLiqiduity call.
        bytes memory data = abi.encodeWithSelector(
            REMOVE_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [
                collWithdrawSize,
                0,
                (stableTokenDebtAmount * withdrawShareAmount) /
                totalShareAmount,
                (assetTokenDebtAmount * withdrawShareAmount) /
                totalShareAmount,
                0,
                0,
                0
            ]
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        // Position equity after removing liquidity
        // Limit the maximum withdrawal amount in a single transaction
        if ((equityBefore - getEquityETHValue()) > OracleLib.getTokenETHValue(oracle, stableToken, maxWithdrawPerTx, msg.sender)) {
            revert Vault_Limit_Exceeded();
        }

        // Calculate token disbursement amount.
        uint256[3] memory withdrawAmounts = [
            // Stable token withdraw amount
            (10000 - feeConfig.withdrawFee) * IERC20(stableToken).balanceOf(address(this)) / 10000,
            // Asset token withdraw amount
            (10000 - feeConfig.withdrawFee) * IERC20(assetToken).balanceOf(address(this)) / 10000,
            // AVAX withdraw amount
            (10000 - feeConfig.withdrawFee) * address(this).balance / 10000
        ];

        // Slippage control
        if (withdrawAmounts[0] < minStableReceived) {
            revert Insufficient_Token_Withdrawn();
        }
        // WAVAX is refunded as native AVAX by Homora's Spell
        if (assetToken == router.WAVAX()) {
            if (withdrawAmounts[2] < minAssetReceived) {
                revert Insufficient_Token_Withdrawn();
            }
        } else if (withdrawAmounts[1] < minAssetReceived) {
            revert Insufficient_Token_Withdrawn();
        }

        // Update position info.
        positions[position_info.chainId][position_info.positionId]
            .shareAmount -= withdrawShareAmount;
        totalShareAmount -= withdrawShareAmount;

        // Transfer fund to user (caller).
        IERC20(stableToken).transfer(recipient, withdrawAmounts[0]);
        IERC20(assetToken).transfer(recipient, withdrawAmounts[1]);
        payable(recipient).transfer(withdrawAmounts[2]);

        // Collect withdrawal fee
        IERC20(stableToken).transfer(feeCollector, IERC20(stableToken).balanceOf(address(this)));
        IERC20(assetToken).transfer(feeCollector, IERC20(assetToken).balanceOf(address(this)));
        payable(feeCollector).transfer(address(this).balance);

        // Check if the PDN position is still healthy
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        // Emit event.
        emit LogWithdraw(
            recipient,
            withdrawShareAmount,
            withdrawAmounts[0],
            withdrawAmounts[1],
            withdrawAmounts[2]
        );
    }

    function collectManagementFee() internal {
        uint256 shareAmtMint = feeConfig.managementFee * (block.timestamp - lastCollectionTimestamp) * totalShareAmount / 31536000 / 10000;
        lastCollectionTimestamp = block.timestamp;
    }

    /// @dev Check if the farming position is delta neutral
    function isDeltaNeutral() public view returns (bool) {
        // Assume token A is the stable token
        VaultLib.VaultPosition memory pos = getPositionInfo();
//        console.log("n_B", pos.amtB, "d_B", pos.debtAmtB);
//        console.log("delta", VaultLib.getOffset(pos.amtB, pos.debtAmtB));
        return (VaultLib.getOffset(pos.amtB, pos.debtAmtB) < deltaThreshold);
    }

    function isDebtRatioHealthy() public view returns (bool) {
        if (homoraBankPosId == _NO_ID) {
            return true;
        } else {
            uint256 debtRatio = getDebtRatio();
            return (minDebtRatio < debtRatio) && (debtRatio < maxDebtRatio);
        }
    }

    /// @dev Rebalance Homora Bank's farming position if delta is not neutral or debt ratio is not healthy
    /// @param slippage: Slippage on the swap between stable token and asset token, multiplied by 1e4, 0.1% => 10
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function rebalance(uint256 slippage, uint256 minReinvestETH)
        external
        onlyController
    {
        reinvestInternal(minReinvestETH);

        uint256 debtRatio = getDebtRatio();
        console.log("Debt ratio before: ", debtRatio);
        console.log("isDeltaNeutral: ", isDeltaNeutral());
        console.log("isDebtRatioHealthy: ", isDebtRatioHealthy());

        // Check if the PDN position need rebalance
        if (isDeltaNeutral() && isDebtRatioHealthy()) {
            revert HomoraPDNVault_PositionIsHealthy();
        }

        // Equity before rebalance
        uint256 equityBefore = getEquityETHValue();

        VaultLib.VaultPosition memory pos = getPositionInfo();
        // 1. short: amtB < debtAmtB, R > Rt, swap A to B
        if (pos.debtAmtB > pos.amtB) {
            reBalanceShort(pos, slippage);
        }
        // 2. long: amtB > debtAmtB, R < Rt, swap B to A
        else {
            rebalanceLong(pos, slippage);
        }

        console.log("Debt ratio after: ", getDebtRatio());

        // Check if the rebalance succeeded
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        emit LogRebalance(equityBefore, getEquityETHValue());
    }

    /// @dev Rebalance when the amount of token B in the LP is less than the amount of debt in token B
    /// @param pos: Farming position in Homora Bank
    /// @param slippage: Slippage on the swap between stable token and asset token, multiplied by 1e4, 0.1% => 10
    function reBalanceShort(
        VaultLib.VaultPosition memory pos,
        uint256 slippage
    ) internal {
        (uint256 reserveA, uint256 reserveB) = getReserves();

        (
            uint256 collWithdrawAmt,
            uint256 amtARepay,
            uint256 amtBRepay,
            uint256 amtASwap,
            uint256 amtBSwap
        ) = VaultLib.rebalanceShort(pos, leverageLevel, reserveA, reserveB);

        // Homora's Spell swaps token A to token B
        uint256 valueBeforeSwap = OracleLib.getTokenETHValue(oracle, stableToken, amtASwap, msg.sender);
        uint256 valueAfterSwap = OracleLib.getTokenETHValue(oracle, assetToken, amtBSwap, msg.sender);
        if (valueBeforeSwap > valueAfterSwap && VaultLib.getOffset(valueAfterSwap, valueBeforeSwap) > slippage) {
            revert Slippage_Too_Large();
        }

        bytes memory data = abi.encodeWithSelector(
            REMOVE_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [collWithdrawAmt, 0, amtARepay, amtBRepay, 0, 0, 0]
        );

        homoraBank.execute(homoraBankPosId, spell, data);
    }

    /// @dev Rebalance when the amount of token B in the LP is greater than the amount of debt in token B
    /// @param pos: Farming position in Homora Bank
    /// @param slippage: Slippage on the swap between stable token and asset token, multiplied by 1e4, 0.1% => 10
    function rebalanceLong(
        VaultLib.VaultPosition memory pos,
        uint256 slippage
    ) internal {
        (uint256 reserveA, uint256 reserveB) = getReserves();
        uint256 amtAReward = IERC20(stableToken).balanceOf(address(this));

        (
            uint256 amtABorrow,
            uint256 amtBBorrow,
            uint256 amtASwap,
            uint256 amtBSwap
        ) = VaultLib.rebalanceLong(
                pos,
                leverageLevel,
                reserveA,
                reserveB,
                amtAReward
            );

        // Homora's Spell swaps token B to token A
        uint256 valueBeforeSwap = OracleLib.getTokenETHValue(oracle, assetToken, amtBSwap, msg.sender);
        uint256 valueAfterSwap = OracleLib.getTokenETHValue(oracle, stableToken, amtASwap, msg.sender);
        if (valueBeforeSwap > valueAfterSwap && VaultLib.getOffset(valueAfterSwap, valueBeforeSwap) > slippage) {
            revert Slippage_Too_Large();
        }

        IERC20(stableToken).approve(address(homoraBank), VaultLib.MAX_UINT);
        bytes memory data = abi.encodeWithSelector(
            ADD_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [amtAReward, 0, 0, amtABorrow, amtBBorrow, 0, 0, 0],
            pid
        );
        homoraBank.execute(homoraBankPosId, spell, data);
        IERC20(stableToken).approve(address(homoraBank), 0);
    }

    /// @dev Collect reward tokens and reinvest
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function reinvest(uint256 minReinvestETH) external onlyController {
        reinvestInternal(minReinvestETH);
    }

    function harvest() internal {
        homoraBank.execute(homoraBankPosId, spell, HARVEST_DATA);
    }

    /// @dev Internal reinvest function
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function reinvestInternal(uint256 minReinvestETH) internal {
        // Position nonexistent
        if (homoraBankPosId == _NO_ID || totalShareAmount == 0) {
            return;
        }

        uint256 equityBefore = getEquityETHValue();

        // 1. Claim rewards
        harvest();
        swapReward();

        // 2. Swap any AVAX leftover
        uint256 avaxBalance = address(this).balance;
        if (avaxBalance > 0) {
            swapAVAX(avaxBalance, stableToken);
        }

        // 3. Reinvest with the current balance
        uint256 stableTokenBalance = IERC20(stableToken).balanceOf(address(this));
        uint256 assetTokenBalance = IERC20(assetToken).balanceOf(address(this));
        console.log("reinvest stableTokenBalance", stableTokenBalance);
        console.log("reinvest assetTokenBalance", assetTokenBalance);

//        (uint256 reserve0, uint256 reserve1) = getReserves();
//        uint256 liquidity = (((stableTokenBalance * leverageLevel) / 2) *
//            IERC20(lpToken).totalSupply()) / reserve0;
//        console.log("liquidity", liquidity);
//        require(liquidity > 0, "Insufficient liquidity minted");

        uint256 stableTokenBorrowAmount;
        uint256 assetTokenBorrowAmount;
        (
            stableTokenBalance,
            assetTokenBalance,
            stableTokenBorrowAmount,
            assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance);
        console.log("reinvest stableTokenBorrowAmount", stableTokenBorrowAmount);
        console.log("reinvest assetTokenBorrowAmount", assetTokenBorrowAmount);

//        (
//            uint256 stableTokenBorrowAmount,
//            uint256 assetTokenBorrowAmount
//        ) = VaultLib.deltaNeutral(
//            stableTokenBalance,
//            assetTokenBalance,
//            reserve0,
//            reserve1,
//            leverageLevel
//        );

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), VaultLib.MAX_UINT);
        IERC20(assetToken).approve(address(homoraBank), VaultLib.MAX_UINT);

        // Encode the calling function.
        bytes memory data = abi.encodeWithSelector(
            ADD_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [
                stableTokenBalance,
                assetTokenBalance,
                0,
                stableTokenBorrowAmount,
                assetTokenBorrowAmount,
                0,
                0,
                0
            ],
            pid
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        // Cancel HomoraBank's allowance.
        IERC20(stableToken).approve(address(homoraBank), 0);
        IERC20(assetToken).approve(address(homoraBank), 0);

        uint256 equityAfter = getEquityETHValue();
        console.log("equity before reinvest", equityBefore);
        console.log("equity after  reinvest", equityAfter);

        if (equityAfter < equityBefore + minReinvestETH) {
            if (
                VaultLib.getOffset(
                    equityAfter,
                    equityBefore + minReinvestETH
                ) >= 10
            ) {
                revert Insufficient_Liquidity_Mint();
            }
        }
        emit LogReinvest(equityBefore, equityAfter);
    }

    /// @dev Homora position info
    function getPositionInfo()
        internal
        view
        returns (VaultLib.VaultPosition memory pos)
    {
        pos.collateralSize = getCollateralSize();
        (pos.amtA, pos.amtB) = convertCollateralToTokens(pos.collateralSize);
        (pos.debtAmtA, pos.debtAmtB) = currentDebtAmount();
        return pos;
    }

    /// @notice Swap fromToken into toToken
    function swap(
        uint256 amount,
        address fromToken,
        address toToken
    )
//        internal
        public
        returns (uint256)
    {
        address[] memory path = new address[](2);
        (path[0], path[1]) = (fromToken, toToken);
        IERC20(fromToken).approve(address(router), amount);
        uint256[] memory resAmt = router.swapExactTokensForTokens(
            amount,
            0,
            path,
            address(this),
            block.timestamp
        );
        IERC20(fromToken).approve(address(router), 0);
        return resAmt[1];
    }

    /// @notice Swap native AVAX into toToken
    function swapAVAX(uint256 amount, address toToken)
//        internal
        public
        returns (uint256)
    {
        address fromToken = router.WAVAX();
        address[] memory path = new address[](2);
        (path[0], path[1]) = (fromToken, toToken);
        uint256[] memory amounts = router.getAmountsOut(amount, path);
        // Reverted by TraderJoe if amounts[1] == 0
        console.log("AmountOut", amounts[1]);
        if (amounts[1] > 0) {
            amounts = router.swapExactAVAXForTokens{value: amount}(
                0,
                path,
                address(this),
                block.timestamp
            );
        }
        return amounts[1];
    }

    /// @notice Swap reward tokens into stable tokens
    function swapReward() internal {
        uint256 rewardAmt = IERC20(rewardToken).balanceOf(address(this));
        if (rewardAmt > 0) {
            uint256 stableRecv = swap(rewardAmt, rewardToken, stableToken);
            uint256 harvestFeeAmt = feeConfig.harvestFee * stableRecv / 10000;
            if (harvestFeeAmt > 0) {
                IERC20(stableToken).safeTransfer(
                    feeCollector,
                    harvestFeeAmt
                );
            }
        }
    }

    /// @notice Get the amount of each of the two tokens in the pool. Stable token first
    function getReserves() internal view returns (uint256, uint256) {
        return VaultLib.getReserves(lpToken, stableToken);
    }

    /// @dev Query the current debt amount for both tokens. Stable first
    function currentDebtAmount() public view returns (uint256, uint256) {
        if (homoraBankPosId == _NO_ID) {
            return (0, 0);
        } else {
            uint256 stableTokenDebtAmount;
            uint256 assetTokenDebtAmount;
            (address[] memory tokens, uint256[] memory debts) = homoraBank
                .getPositionDebts(homoraBankPosId);
            for (uint256 i = 0; i < tokens.length; i++) {
                if (tokens[i] == stableToken) {
                    stableTokenDebtAmount = debts[i];
                } else if (tokens[i] == assetToken) {
                    assetTokenDebtAmount = debts[i];
                }
            }
            return (stableTokenDebtAmount, assetTokenDebtAmount);
        }
    }

    /// @dev Total position value, not weighted by the collateral factor
    function getCollateralETHValue()
        public view
        returns (uint256)
    {
        return homoraBankPosId == _NO_ID
            ? 0
            : homoraBank.getCollateralETHValue(homoraBankPosId) * 10**4 / collateralFactor;
    }

    /// @dev Total debt value, not weighted by the borrow factors
    function getBorrowETHValue()
        public view
        returns (uint256)
    {
        (uint256 stableTokenDebtAmount, uint256 assetTokenDebtAmount) = currentDebtAmount();
        return (homoraBankPosId == _NO_ID)
            ? 0
            // change msg.sender to adapter
            : OracleLib.getTokenETHValue(oracle, stableToken, stableTokenDebtAmount, msg.sender)
                + OracleLib.getTokenETHValue(oracle, assetToken, assetTokenDebtAmount, msg.sender);
    }

    /// @dev Net equity value of the PDN position
    function getEquityETHValue() public view returns (uint256) {
        return getCollateralETHValue() - getBorrowETHValue();
    }

    /// @notice Calculate the debt ratio and return the ratio, multiplied by 1e4
    function getDebtRatio() public view returns (uint256) {
        require(homoraBankPosId != _NO_ID, "Invalid Homora Bank position id");
        uint256 collateralValue = homoraBank.getCollateralETHValue(
            homoraBankPosId
        );
        uint256 borrowValue = homoraBank.getBorrowETHValue(homoraBankPosId);
        return (borrowValue * 10000) / collateralValue;
    }

    function getCollateralSize() public view returns (uint256) {
        if (homoraBankPosId == _NO_ID) return 0;
        (, , , uint256 collateralSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        return collateralSize;
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens. Stable token first
    /// @param collAmount: Amount of LP token
    function convertCollateralToTokens(uint256 collAmount)
        public
        view
        returns (uint256, uint256)
    {
        return
            VaultLib.convertCollateralToTokens(
                lpToken,
                stableToken,
                collAmount
            );
    }

    /// @dev Query a user position's share
    /// @param position_info: Aperture position info
    function getShareAmount(PositionInfo memory position_info)
        public
        view
        returns (uint256)
    {
        return
            positions[position_info.chainId][position_info.positionId]
                .shareAmount;
    }
}
