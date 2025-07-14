// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import {IFlashLoanReceiver} from "@aave/core-v3/contracts/flashloan/interfaces/IFlashLoanReceiver.sol";
import {IPoolAddressesProvider} from "@aave/core-v3/contracts/interfaces/IPoolAddressesProvider.sol";
import {IPool} from "@aave/core-v3/contracts/interfaces/IPool.sol";
import {IL2Pool} from "@aave/core-v3/contracts/interfaces/IL2Pool.sol";
import {IPoolDataProvider} from "@aave/core-v3/contracts/interfaces/IPoolDataProvider.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import {ISwapRouter} from "@uniswap/v3-periphery/contracts/interfaces/ISwapRouter.sol";

/**
 * @title AaveLiquidator
 * @notice Flash loan liquidation contract for Aave v3 on Base
 * @dev Implements IFlashLoanReceiver to perform atomic liquidations with flash loans
 */
contract AaveLiquidator is IFlashLoanReceiver, Ownable, ReentrancyGuard {
    using SafeERC20 for IERC20;

    // Network-specific addresses - configurable at deployment
    address private immutable POOL_ADDRESS;
    address private immutable ADDRESSES_PROVIDER_ADDRESS;
    address private immutable SWAP_ROUTER;
    address private immutable DATA_PROVIDER;

    /* Network Address Reference:
     * Base Mainnet:
     *   - Pool: 0xA238Dd80C259a72e81d7e4664a9801593F98d1c5
     *   - AddressesProvider: 0x2f39d218133AFaB8F2B819B1066c7E434Ad94E9e
     *   - SwapRouter: 0x2626664c2603336E57B271c5C0b26F421741e481
     *
     * Base Sepolia Testnet:
     *   - Pool: 0xA37D7E3d3CaD89b44f9a08A96fE01a9F39Bd7794
     *   - AddressesProvider: 0x0D8176C0e8965F2730c4C1aA5aAE816fE4b7a802
     *   - SwapRouter: 0x8357227D4eDd91C4f85615C9cC5761899CD4B068
     */

    // Maximum slippage tolerance (5% in basis points)
    uint256 public constant MAX_SLIPPAGE = 500;

    // Minimum profit threshold in USD (scaled by 1e8 to match oracle precision)
    uint256 public minProfitThreshold = 5 * 1e8; // $5

    struct LiquidationParams {
        address user;
        address collateralAsset;
        address debtAsset;
        uint256 debtToCover;
        bool receiveAToken;
        uint256 collateralAssetId;
        uint256 debtAssetId;
    }

    event LiquidationExecuted(
        address indexed user,
        address indexed collateralAsset,
        address indexed debtAsset,
        uint256 debtCovered,
        uint256 collateralReceived,
        uint256 profit
    );

    event ProfitWithdrawn(
        address indexed asset,
        uint256 amount,
        address indexed to
    );

    constructor(
        address _poolAddress,
        address _addressesProviderAddress,
        address _swapRouter
    ) Ownable() {
        require(_poolAddress != address(0), "Invalid pool address");
        require(
            _addressesProviderAddress != address(0),
            "Invalid addresses provider"
        );
        require(_swapRouter != address(0), "Invalid swap router address");

        POOL_ADDRESS = _poolAddress;
        ADDRESSES_PROVIDER_ADDRESS = _addressesProviderAddress;
        SWAP_ROUTER = _swapRouter;
        address dataProvider = IPoolAddressesProvider(_addressesProviderAddress)
            .getPoolDataProvider();
        require(dataProvider != address(0), "Invalid data provider");
        DATA_PROVIDER = dataProvider;
    }

    // Required by IFlashLoanReceiver
    function POOL() external view returns (IPool) {
        return IPool(POOL_ADDRESS);
    }

    // Required by IFlashLoanReceiver
    function ADDRESSES_PROVIDER()
        external
        view
        returns (IPoolAddressesProvider)
    {
        return IPoolAddressesProvider(ADDRESSES_PROVIDER_ADDRESS);
    }

    /**
     * @notice Initiates a flash loan liquidation
     * @param user The user to liquidate
     * @param collateralAsset The collateral asset to seize
     * @param debtAsset The debt asset to repay
     * @param debtToCover Amount of debt to cover (use type(uint256).max for maximum)
     * @param receiveAToken Whether to receive aTokens or underlying assets
     * @param collateralAssetId Asset ID for L2Pool encoding
     * @param debtAssetId Asset ID for L2Pool encoding
     */
    function liquidate(
        address user,
        address collateralAsset,
        address debtAsset,
        uint256 debtToCover,
        bool receiveAToken,
        uint16 collateralAssetId,
        uint16 debtAssetId
    ) external onlyOwner nonReentrant {
        // Prepare liquidation parameters
        LiquidationParams memory params = LiquidationParams({
            user: user,
            collateralAsset: collateralAsset,
            debtAsset: debtAsset,
            debtToCover: debtToCover,
            receiveAToken: receiveAToken,
            collateralAssetId: collateralAssetId,
            debtAssetId: debtAssetId
        });

        // Encode parameters for flash loan callback
        bytes memory paramsBytes = abi.encode(params);

        // Determine actual debt amount to cover
        uint256 actualDebtToCover = debtToCover;
        if (debtToCover == type(uint256).max) {
            uint256 assetDebt = _getUserAssetDebt(debtAsset, user);
            actualDebtToCover = assetDebt / 2;
        }

        // Request flash loan
        address[] memory assets = new address[](1);
        assets[0] = debtAsset;

        uint256[] memory amounts = new uint256[](1);
        amounts[0] = actualDebtToCover;

        uint256[] memory modes = new uint256[](1);
        modes[0] = 0; // No debt, pay back immediately

        IPool(POOL_ADDRESS).flashLoan(
            address(this),
            assets,
            amounts,
            modes,
            address(this),
            paramsBytes,
            0 // referral code
        );
    }

    /**
     * @notice Called by Aave Pool after flash loan is granted
     * @param assets The assets flash loaned
     * @param amounts The amounts flash loaned
     * @param premiums The flash loan fees
     * @param initiator The address that initiated the flash loan
     * @param params Encoded liquidation parameters
     */
    function executeOperation(
        address[] calldata assets,
        uint256[] calldata amounts,
        uint256[] calldata premiums,
        address initiator,
        bytes calldata params
    ) external override returns (bool) {
        require(msg.sender == POOL_ADDRESS, "Caller must be Aave Pool");
        require(initiator == address(this), "Invalid initiator");

        // Decode parameters
        LiquidationParams memory liquidationParams = abi.decode(
            params,
            (LiquidationParams)
        );

        address debtAsset = assets[0];
        uint256 amount = amounts[0];
        uint256 premium = premiums[0];

        // Approve Pool to spend debt asset for liquidation
        IERC20(debtAsset).safeApprove(POOL_ADDRESS, amount);

        // Execute liquidation using L2Pool for gas efficiency
        _executeLiquidation(liquidationParams, amount);

        // Calculate collateral received
        uint256 collateralBalance = IERC20(liquidationParams.collateralAsset)
            .balanceOf(address(this));

        // Swap collateral to debt asset if they're different
        uint256 debtAssetBalance = IERC20(debtAsset).balanceOf(address(this));
        if (
            liquidationParams.collateralAsset != debtAsset &&
            collateralBalance > 0
        ) {
            debtAssetBalance += _swapCollateralToDebt(
                liquidationParams.collateralAsset,
                debtAsset,
                collateralBalance
            );
        }

        // Calculate total amount to repay (principal + premium)
        uint256 totalRepayAmount = amount + premium;

        // Ensure we have enough to repay the flash loan
        require(
            debtAssetBalance >= totalRepayAmount,
            "Insufficient funds to repay flash loan"
        );

        // Approve Pool to collect the repayment
        IERC20(debtAsset).safeApprove(POOL_ADDRESS, totalRepayAmount);

        // Calculate and emit profit
        uint256 profit = debtAssetBalance - totalRepayAmount;

        emit LiquidationExecuted(
            liquidationParams.user,
            liquidationParams.collateralAsset,
            debtAsset,
            amount,
            collateralBalance,
            profit
        );

        return true;
    }

    /**
     * @notice Execute the actual liquidation using L2Pool
     * @param params Liquidation parameters
     * @param debtToCover Amount of debt to cover
     */
    function _executeLiquidation(
        LiquidationParams memory params,
        uint256 debtToCover
    ) internal {
        // Encode parameters for L2Pool liquidation call
        // args1: collateralAssetId (16 bits) + debtAssetId (16 bits) + user address (160 bits)
        bytes32 args1 = bytes32(
            (uint256(params.collateralAssetId) << 240) |
                (uint256(params.debtAssetId) << 224) |
                uint256(uint160(params.user))
        );

        // args2: debtToCover (128 bits) + receiveAToken flag (1 bit)
        bytes32 args2 = bytes32(
            (debtToCover << 128) | (params.receiveAToken ? 1 : 0)
        );

        // Call L2Pool liquidation function
        IL2Pool(POOL_ADDRESS).liquidationCall(args1, args2);
    }

    /// @notice Retrieve total stable and variable debt for a user's asset
    function _getUserAssetDebt(address asset, address user)
        internal
        view
        returns (uint256)
    {
        (
            ,
            uint256 stableDebt,
            uint256 variableDebt,
            ,
            ,
            ,
            ,
            ,
        ) = IPoolDataProvider(DATA_PROVIDER).getUserReserveData(asset, user);
        return stableDebt + variableDebt;
    }

    /**
     * @notice Swap collateral asset to debt asset using Uniswap V3
     * @param collateralAsset The asset to swap from
     * @param debtAsset The asset to swap to
     * @param amountIn The amount to swap
     * @return amountOut The amount received
     */
    function _swapCollateralToDebt(
        address collateralAsset,
        address debtAsset,
        uint256 amountIn
    ) internal returns (uint256 amountOut) {
        // Approve Uniswap router to spend collateral
        IERC20(collateralAsset).safeApprove(SWAP_ROUTER, amountIn);

        // Calculate minimum amount out (with slippage protection)
        // This is a simplified calculation - in production, you'd use a price oracle
        uint256 amountOutMinimum = (amountIn * (10000 - MAX_SLIPPAGE)) / 10000;

        // Set up swap parameters
        ISwapRouter.ExactInputSingleParams memory swapParams = ISwapRouter
            .ExactInputSingleParams({
                tokenIn: collateralAsset,
                tokenOut: debtAsset,
                fee: 3000, // 0.3% fee tier
                recipient: address(this),
                deadline: block.timestamp + 300, // 5 minutes
                amountIn: amountIn,
                amountOutMinimum: amountOutMinimum,
                sqrtPriceLimitX96: 0
            });

        // Execute the swap
        amountOut = ISwapRouter(SWAP_ROUTER).exactInputSingle(swapParams);

        // Reset approval
        IERC20(collateralAsset).safeApprove(SWAP_ROUTER, 0);
    }

    /**
     * @notice Withdraw accumulated profits
     * @param asset The asset to withdraw
     * @param amount The amount to withdraw (0 for full balance)
     * @param to The address to send the assets to
     */
    function withdraw(
        address asset,
        uint256 amount,
        address to
    ) external onlyOwner {
        require(to != address(0), "Invalid recipient");

        uint256 balance = IERC20(asset).balanceOf(address(this));
        uint256 withdrawAmount = amount == 0 ? balance : amount;

        require(withdrawAmount <= balance, "Insufficient balance");
        require(withdrawAmount > 0, "Nothing to withdraw");

        IERC20(asset).safeTransfer(to, withdrawAmount);

        emit ProfitWithdrawn(asset, withdrawAmount, to);
    }

    /**
     * @notice Withdraw ETH accumulated in the contract
     * @param to The address to send ETH to
     */
    function withdrawETH(address payable to) external onlyOwner {
        require(to != address(0), "Invalid recipient");
        uint256 balance = address(this).balance;
        require(balance > 0, "No ETH to withdraw");

        (bool success, ) = to.call{value: balance}("");
        require(success, "ETH transfer failed");
    }

    /**
     * @notice Set minimum profit threshold
     * @param newThreshold New threshold in USD (scaled by 1e8)
     */
    function setMinProfitThreshold(uint256 newThreshold) external onlyOwner {
        minProfitThreshold = newThreshold;
    }

    /**
     * @notice Emergency function to approve tokens (if needed)
     * @param token The token to approve
     * @param spender The spender to approve
     * @param amount The amount to approve
     */
    function emergencyApprove(
        address token,
        address spender,
        uint256 amount
    ) external onlyOwner {
        IERC20(token).safeApprove(spender, amount);
    }

    /**
     * @notice Get the Aave Pool address
     */
    function getPool() external view returns (address) {
        return POOL_ADDRESS;
    }

    /**
     * @notice Get the Uniswap V3 SwapRouter address
     */
    function getSwapRouter() external view returns (address) {
        return SWAP_ROUTER;
    }

    /**
     * @notice Get the Aave AddressesProvider address
     */
    function getAddressesProvider() external view returns (address) {
        return ADDRESSES_PROVIDER_ADDRESS;
    }

    /**
     * @notice Get the Aave Protocol DataProvider address
     */
    function getDataProvider() external view returns (address) {
        return DATA_PROVIDER;
    }

    // Allow contract to receive ETH
    receive() external payable {}
}
