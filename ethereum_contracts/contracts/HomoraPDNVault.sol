//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/math/Math.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/IHomoraAvaxRouter.sol";
import "./interfaces/IHomoraBank.sol";
import "./interfaces/IHomoraSpell.sol";

import "./libraries/VaultLib.sol";

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
    using Math for uint256;

    struct ApertureFeeConfig {
        uint256 withdrawFee; // multiplied by 1e4
        uint256 harvestFee; // multiplied by 1e4
        uint256 managementFee; // multiplied by 1e4
    }

    struct ApertureVaultLimits {
        uint256 maxCapacity; // Maximum amount allowed in stable across the vault
        uint256 maxOpenPerTx; // Maximum amount allowed in stable to add in one transaction
        uint256 maxWithdrawPerTx; // Maximum amount allowed in stable to withdraw in one transaction
    }

    // --- modifiers ---
    modifier onlyApertureManager() {
        require(msg.sender == apertureManager, "unauthorized mgr op");
        _;
    }

    modifier onlyController() {
        require(isController[msg.sender], "unauthorized controller");
        _;
    }

    // --- constants ---
    address public WAVAX;

    // --- accounts ---
    address public apertureManager;
    address public feeCollector;
    mapping(address => bool) public isController;

    // --- config ---
    ContractInfo public contractInfo;
    // .adapter: Immutable adapter to HomoraBank
    // .bank: HomoraBank's address
    // .oracle: HomoraBank's oracle for determining prices
    // .router: TraderJoe's router address
    // .spell: Homora's Spell contract address
    PairInfo public pairInfo; // token 0 address, token 1 address, ERC-20 LP token address
    uint256 public leverageLevel; // target leverage
    uint256 public pid; // pool id

    uint256 public collateralFactor; // LP collateral factor on Homora
    uint256 public stableBorrowFactor; // stable token borrow factor on Homora
    uint256 public assetBorrowFactor; // asset token borrow factor on Homora
    uint256 public targetDebtRatio; // target debt ratio * 10000, 92% -> 9200
    uint256 public minDebtRatio; // minimum debt ratio * 10000
    uint256 public maxDebtRatio; // maximum debt ratio * 10000
    uint256 public deltaThreshold; // delta deviation threshold in percentage * 10000

    ApertureVaultLimits public vaultLimits;
    ApertureFeeConfig public feeConfig;

    // --- state ---
    // positions[chainId][positionId] stores share information about the position identified by (chainId, positionId).
    mapping(uint16 => mapping(uint128 => Position)) public positions;
    // Position id of the PDN vault in HomoraBank. Zero for new position.
    uint256 public homoraPosId;
    VaultState public vaultState;

    // --- event ---
    event LogDeposit(
        uint16 indexed chainId,
        uint128 indexed positionId,
        uint256 equityAmount,
        uint256 shareAmount
    );
    event LogWithdraw(
        uint16 indexed recipientChainId,
        bytes32 indexed recipientAddr,
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

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        address _apertureManager,
        address _adapter,
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
        setFeeCollector(_feeCollector);
        isController[_controller] = true;
        contractInfo.adapter = _adapter;
        contractInfo.bank = _homoraBank;
        contractInfo.oracle = IHomoraBank(_homoraBank).oracle();
        contractInfo.spell = _spell;
        contractInfo.router = IHomoraSpell(_spell).router();
        WAVAX = IHomoraAvaxRouter(contractInfo.router).WAVAX();
        require(VaultLib.support(contractInfo.oracle, _stableToken));
        require(VaultLib.support(contractInfo.oracle, _assetToken));
        pairInfo.stableToken = _stableToken;
        pairInfo.assetToken = _assetToken;
        pairInfo.rewardToken = _rewardToken;

        pid = _pid;
        homoraPosId = VaultLib._NO_ID;
        pairInfo.lpToken = IHomoraSpell(_spell).pairs(
            pairInfo.stableToken,
            pairInfo.assetToken
        );
        require(pairInfo.lpToken != address(0));
        require(VaultLib.supportLP(contractInfo.oracle, pairInfo.lpToken));
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    /// @dev Set config for delta neutral valut.
    /// @param _leverageLevel: Target leverage
    /// @param _targetDebtRatio: Target debt ratio * 10000
    /// @param _debtRatioWidth: Deviation of debt ratio * 10000
    /// @param _deltaThreshold: Delta deviation threshold in percentage * 10000
    /// @param _feeConfig: Farming reward fee, withdrawal fee and management fee
    /// @param _vaultLimits: Max vault size, max amount per open and max amount per withdrawal
    function initializeConfig(
        uint256 _leverageLevel,
        uint256 _targetDebtRatio,
        uint256 _debtRatioWidth,
        uint256 _deltaThreshold,
        ApertureFeeConfig calldata _feeConfig,
        ApertureVaultLimits calldata _vaultLimits
    ) external onlyOwner {
        setConfig(
            _leverageLevel,
            _targetDebtRatio,
            _debtRatioWidth,
            _deltaThreshold
        );
        setFees(_feeConfig);
        setVaultLimits(_vaultLimits);
    }

    function setControllers(
        address[] calldata controllers,
        bool[] calldata statuses
    ) external onlyOwner {
        require(controllers.length == statuses.length);
        for (uint256 i = 0; i < controllers.length; i++) {
            isController[controllers[i]] = statuses[i];
        }
    }

    function setAdapter(address _adapter) external onlyOwner {
        contractInfo.adapter = _adapter;
    }

    function setFeeCollector(address _feeCollector) public onlyOwner {
        feeCollector = _feeCollector;
    }

    /// @param _feeConfig: Farming reward fee, withdrawal fee and management fee
    function setFees(ApertureFeeConfig calldata _feeConfig) public onlyOwner {
        feeConfig = _feeConfig;
    }

    /// @param _vaultLimits: Max vault size, max amount per open and max amount per withdrawal
    function setVaultLimits(ApertureVaultLimits calldata _vaultLimits)
        public
        onlyOwner
    {
        vaultLimits = _vaultLimits;
    }

    /// @dev Set config for pseudo delta-neutral valut.
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
        require(_leverageLevel >= 2);
        leverageLevel = _leverageLevel;
        collateralFactor = VaultLib.getCollateralFactor(
            contractInfo.oracle,
            pairInfo.lpToken
        );
        stableBorrowFactor = VaultLib.getBorrowFactor(
            contractInfo.oracle,
            pairInfo.stableToken
        );
        assetBorrowFactor = VaultLib.getBorrowFactor(
            contractInfo.oracle,
            pairInfo.assetToken
        );
        uint256 calculatedDebtRatio = VaultLib.calculateDebtRatio(
            leverageLevel,
            collateralFactor,
            stableBorrowFactor,
            assetBorrowFactor
        );
        require(
            VaultLib.abs(_targetDebtRatio, calculatedDebtRatio) <= 10,
            "Invalid debt ratio"
        );
        targetDebtRatio = _targetDebtRatio;
        minDebtRatio = targetDebtRatio - _debtRatioWidth;
        maxDebtRatio = targetDebtRatio + _debtRatioWidth;
        require(0 < minDebtRatio && maxDebtRatio < 10000);

        uint256 calculatedDeltaTh = VaultLib.calculateDeltaThreshold(
            leverageLevel,
            _debtRatioWidth,
            collateralFactor,
            stableBorrowFactor,
            assetBorrowFactor
        );
        require(
            VaultLib.abs(_deltaThreshold, calculatedDeltaTh) <= 10,
            "Invalid delta threshold"
        );
        deltaThreshold = _deltaThreshold;
    }

    receive() external payable {}

    /// @dev Open a new Aperture position.
    /// @param position_info: Aperture position info.
    /// @param assets: At most two assets, one `stableToken`, and the other `assetToken`.
    /// @param data: Encoded (uint256 minEquityETH, uint256 minReinvestETH).
    function openPosition(
        PositionInfo memory position_info,
        AssetInfo[] calldata assets,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        increasePositionInternal(position_info, assets, data);
    }

    /// @dev Increase an existing Aperture position.
    /// @param position_info: Aperture position info.
    /// @param assets: At most two assets, one `stableToken`, and the other `assetToken`.
    /// @param data: Encoded (uint256 minEquityETH, uint256 minReinvestETH).
    function increasePosition(
        PositionInfo memory position_info,
        AssetInfo[] calldata assets,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        increasePositionInternal(position_info, assets, data);
    }

    function increasePositionInternal(
        PositionInfo memory position_info,
        AssetInfo[] calldata assets,
        bytes calldata data
    ) internal {
        uint256 stableTokenDepositAmount = 0;
        uint256 assetTokenDepositAmount = 0;

        for (uint256 i = 0; i < assets.length; ++i) {
            if (assets[i].assetAddr == pairInfo.stableToken) {
                stableTokenDepositAmount += assets[i].amount;
            } else if (assets[i].assetAddr == pairInfo.assetToken) {
                assetTokenDepositAmount += assets[i].amount;
            } else {
                revert("unexpected token");
            }
        }

        (uint256 minEquityETH, uint256 minReinvestETH) = abi.decode(
            data,
            (uint256, uint256)
        );
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
    /// @param data: Encoded (uint256 shareAmount, uint256 amtAMin, uint256 amtBMin, uint256 minReinvestETH).
    function decreasePosition(
        PositionInfo memory position_info,
        Recipient calldata recipient,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (
            uint256 shareAmount,
            uint256 amtAMin,
            uint256 amtBMin,
            uint256 minReinvestETH
        ) = abi.decode(data, (uint256, uint256, uint256, uint256));
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
    /// @param data: Encoded (uint256 amtAMin, uint256 amtBMin, uint256 minReinvestETH).
    function closePosition(
        PositionInfo memory position_info,
        Recipient calldata recipient,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (
            uint256 amtAMin,
            uint256 amtBMin,
            uint256 minReinvestETH
        ) = abi.decode(data, (uint256, uint256, uint256));
        withdrawInternal(
            position_info,
            recipient,
            getShareAmount(position_info),
            amtAMin,
            amtBMin,
            minReinvestETH
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
        reinvestInternal(minReinvestETH);

        // Check if the PDN position need rebalance
        if (!isDeltaNeutral() || !isDebtRatioHealthy()) {
            rebalanceInternal(10);
        }

        VaultLib.collectManagementFee(
            positions,
            vaultState,
            feeConfig.managementFee
        );

        // Transfer user's deposit tokens to current contract.
        if (stableTokenDepositAmount > 0) {
            IERC20(pairInfo.stableToken).safeTransferFrom(
                msg.sender,
                address(this),
                stableTokenDepositAmount
            );
        }
        if (assetTokenDepositAmount > 0) {
            IERC20(pairInfo.assetToken).safeTransferFrom(
                msg.sender,
                address(this),
                assetTokenDepositAmount
            );
        }

        // Record original position equity before adding liquidity
        uint256 equityBefore = getEquityETHValue();

        // Add liquidity with the current balance
        homoraPosId = VaultLib.deposit(
            contractInfo,
            homoraPosId,
            pairInfo,
            leverageLevel,
            pid,
            msg.value
        );

        // Position equity after adding liquidity
        uint256 equityAfter = getEquityETHValue();
        uint256 equityChange = equityAfter - equityBefore;
        // Calculate user share amount.
        uint256 shareAmount = equityBefore == 0
            ? equityChange
            : vaultState.totalShareAmount.mulDiv(equityChange, equityBefore);

        if (equityChange < minEquityETH) {
            revert Insufficient_Liquidity_Mint();
        }

        if (
            equityChange >
            getTokenETHValue(pairInfo.stableToken, vaultLimits.maxOpenPerTx)
        ) {
            revert Vault_Limit_Exceeded();
        }

        if (
            equityAfter >
            getTokenETHValue(pairInfo.stableToken, vaultLimits.maxCapacity)
        ) {
            revert Vault_Limit_Exceeded();
        }

        // Update vault position state.
        vaultState.totalShareAmount += shareAmount;

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
        Recipient calldata recipient,
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

        reinvestInternal(minReinvestETH);

        // Check if the PDN position need rebalance
        if (!isDeltaNeutral() || !isDebtRatioHealthy()) {
            rebalanceInternal(10);
        }

        VaultLib.collectManagementFee(
            positions,
            vaultState,
            feeConfig.managementFee
        );

        // Record original position equity before removing liquidity
        uint256 equityBefore = getEquityETHValue();

        // Actual withdraw actions
        uint256[3] memory withdrawAmounts = VaultLib.withdraw(
            contractInfo,
            homoraPosId,
            pairInfo,
            VaultLib.SOME_LARGE_NUMBER.mulDiv(
                withdrawShareAmount,
                vaultState.totalShareAmount
            ),
            feeConfig.withdrawFee
        );

        // Update position info.
        positions[position_info.chainId][position_info.positionId]
            .shareAmount -= withdrawShareAmount;
        vaultState.totalShareAmount -= withdrawShareAmount;

        // Transfer fund to the recipient (possibly initiate cross-chain transfer).
        IERC20(pairInfo.stableToken).transfer(apertureManager, withdrawAmounts[0]);
        IERC20(pairInfo.assetToken).transfer(apertureManager, withdrawAmounts[1]);
        AssetInfo[] memory assetInfos = new AssetInfo[](2);
        assetInfos[0] = AssetInfo(pairInfo.stableToken, withdrawAmounts[0]);
        assetInfos[1] = AssetInfo(pairInfo.assetToken, withdrawAmounts[1]);
        IApertureManager(apertureManager).disburseAssets{value: withdrawAmounts[2]}(assetInfos, recipient);

        // Collect withdrawal fee
        IERC20(pairInfo.stableToken).transfer(
            feeCollector,
            IERC20(pairInfo.stableToken).balanceOf(address(this))
        );
        IERC20(pairInfo.assetToken).transfer(
            feeCollector,
            IERC20(pairInfo.assetToken).balanceOf(address(this))
        );
        payable(feeCollector).transfer(address(this).balance);

        // Slippage control
        // WAVAX is refunded as native AVAX by Homora's Spell
        if (pairInfo.stableToken == WAVAX) {
            if (
                withdrawAmounts[0] + withdrawAmounts[2] < minStableReceived ||
                withdrawAmounts[1] < minAssetReceived
            ) {
                revert Insufficient_Token_Withdrawn();
            }
        } else if (pairInfo.assetToken == WAVAX) {
            if (
                withdrawAmounts[0] < minStableReceived ||
                withdrawAmounts[1] + withdrawAmounts[2] < minAssetReceived
            ) {
                revert Insufficient_Token_Withdrawn();
            }
        } else {
            if (
                withdrawAmounts[0] < minStableReceived ||
                withdrawAmounts[1] < minAssetReceived
            ) {
                revert Insufficient_Token_Withdrawn();
            }
        }

        // Check position equity after removing liquidity
        // Limit the maximum withdrawal amount in a single transaction
        if (
            (equityBefore - getEquityETHValue()) >
            getTokenETHValue(pairInfo.stableToken, vaultLimits.maxWithdrawPerTx)
        ) {
            revert Vault_Limit_Exceeded();
        }

        // Check if the PDN position is still healthy
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        // Emit event.
        emit LogWithdraw(
            recipient.chainId,
            recipient.recipientAddr,
            withdrawShareAmount,
            withdrawAmounts[0],
            withdrawAmounts[1],
            withdrawAmounts[2]
        );
    }

    /// @dev Collect reward tokens and reinvest
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function reinvest(uint256 minReinvestETH) external onlyController {
        reinvestInternal(minReinvestETH);
    }

    /// @dev Collect reward tokens and reinvest
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function reinvestInternal(uint256 minReinvestETH) internal {
        // Position nonexistent
        if (
            homoraPosId == VaultLib._NO_ID ||
            vaultState.totalShareAmount == 0
        ) {
            return;
        }

        // 1. Claim rewards and collect harvest fee
        VaultLib.harvest(contractInfo, homoraPosId, pairInfo);

        VaultLib.swapRewardCollectFee(
            contractInfo.router,
            feeCollector,
            pairInfo,
            feeConfig.harvestFee
        );

        // 2. Swap any AVAX leftover
        uint256 avaxBalance = address(this).balance;
        if (avaxBalance > 0) {
            VaultLib.swapAVAX(
                contractInfo.router,
                avaxBalance,
                pairInfo.stableToken
            );
        }

        uint256 stableBalance = IERC20(pairInfo.stableToken).balanceOf(
            address(this)
        );
        console.log("stableBalance", stableBalance);

        uint256 rewardETHValue = getTokenETHValue(pairInfo.stableToken, stableBalance);

        // Not worth the gas
        if (rewardETHValue < 50e15) {
//            IERC20(pairInfo.stableToken).safeTransfer(contractInfo.oracle, stableBalance);
            return;
        }

        if (rewardETHValue < minReinvestETH) {
            revert Insufficient_Liquidity_Mint();
        }

        uint256 equityBefore = getEquityETHValue();

        // 3. Add liquidity with the current balance
        VaultLib.deposit(
            contractInfo,
            homoraPosId,
            pairInfo,
            leverageLevel,
            pid,
            0
        );

        stableBalance = IERC20(pairInfo.stableToken).balanceOf(
            address(this)
        );
        console.log("stableBalance", stableBalance);

        uint256 equityAfter = getEquityETHValue();

        emit LogReinvest(equityBefore, equityAfter);
    }

    /// @dev Rebalance Homora Bank's farming position if delta is not neutral or debt ratio is not healthy
    /// @param slippage: Slippage on the swap between stable token and asset token, multiplied by 1e4, 0.1% => 10
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function rebalance(uint256 slippage, uint256 minReinvestETH)
        external
        onlyController
    {
        reinvestInternal(minReinvestETH);
        rebalanceInternal(slippage);
    }

    /// @dev Internal rebalance function
    /// @param slippage: Slippage in the swap, multiplied by 1e4, 0.1% => 10
    function rebalanceInternal(uint256 slippage) internal {
        // Check if the PDN position need rebalance
        if (isDeltaNeutral() && isDebtRatioHealthy()) {
            revert HomoraPDNVault_PositionIsHealthy();
        }

        // Equity before rebalance
        uint256 equityBefore = getEquityETHValue();

        // Position info in Homora Bank
        VaultPosition memory pos = VaultLib.getPositionInfo(
            contractInfo.bank,
            homoraPosId,
            pairInfo
        );

        // Actual rebalance actions
        if (pos.debtAmtB > pos.amtB) {
            // 1. short: amtB < debtAmtB, R > Rt, swap A to B
            VaultLib.rebalanceShort(
                contractInfo,
                homoraPosId,
                pos,
                pairInfo,
                leverageLevel,
                slippage,
                stableBorrowFactor,
                assetBorrowFactor
            );
        } else {
            // 2. long: amtB > debtAmtB, R < Rt, swap B to A
            VaultLib.rebalanceLong(
                contractInfo,
                homoraPosId,
                pos,
                pairInfo,
                leverageLevel,
                slippage,
                stableBorrowFactor,
                assetBorrowFactor,
                pid
            );
        }

        // Check if the rebalance succeeded
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        emit LogRebalance(equityBefore, getEquityETHValue());
    }

    /// @dev Check if the farming position is delta neutral
    function isDeltaNeutral() internal view returns (bool) {
        return
            VaultLib.isDeltaNeutral(
                contractInfo.bank,
                homoraPosId,
                pairInfo,
                deltaThreshold
            );
    }

    function isDebtRatioHealthy() internal view returns (bool) {
        return
            VaultLib.isDebtRatioHealthy(
                contractInfo.bank,
                homoraPosId,
                minDebtRatio,
                maxDebtRatio
            );
    }

    /// @dev Return the value of the given token as ETH, *not* weighted by the borrow factor. Assume token is supported by the oracle
    function getTokenETHValue(address token, uint256 amount)
        internal
        view
        returns (uint256)
    {
        return
            token == pairInfo.stableToken
                ? VaultLib.getTokenETHValue(
                    contractInfo,
                    pairInfo.stableToken,
                    amount,
                    stableBorrowFactor
                )
                : VaultLib.getTokenETHValue(
                    contractInfo,
                    pairInfo.assetToken,
                    amount,
                    assetBorrowFactor
                );
    }

    /// @dev Total value of the PDN position
    function getCollateralETHValue() public view returns (uint256) {
        return VaultLib.getCollateralETHValue(
                contractInfo.bank,
                homoraPosId,
                collateralFactor
            );
    }

    /// @dev Net equity value of the PDN position
    function getEquityETHValue() public view returns (uint256) {
        return getCollateralETHValue()
            - VaultLib.getBorrowETHValue(
                contractInfo,
                homoraPosId,
                pairInfo,
                stableBorrowFactor,
                assetBorrowFactor
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
