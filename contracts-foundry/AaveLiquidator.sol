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

    address private immutable POOL_ADDRESS;
    address private immutable ADDRESSES_PROVIDER_ADDRESS;
    address private immutable SWAP_ROUTER;
    address private immutable DATA_PROVIDER;

    uint256 public maxSlippage = 500; // 5% default, now configurable
    uint256 public swapDeadlineBuffer = 300; // 5 minutes default, now configurable
    uint24 public defaultSwapFee = 3000; // 0.3% default fee tier, now configurable
    uint256 public minProfitThreshold = 5 * 1e8;

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

        // Hardcoded Aave V3 Base DataProvider address to avoid dynamic lookup revert
        DATA_PROVIDER = 0x2d8A3C5677189723C4cB8873CfC9C8976FDF38Ac;
    }

    function POOL() external view returns (IPool) {
        return IPool(POOL_ADDRESS);
    }

    function ADDRESSES_PROVIDER()
        external
        view
        returns (IPoolAddressesProvider)
    {
        return IPoolAddressesProvider(ADDRESSES_PROVIDER_ADDRESS);
    }

    function liquidate(
        address user,
        address collateralAsset,
        address debtAsset,
        uint256 debtToCover,
        bool receiveAToken,
        uint16 collateralAssetId,
        uint16 debtAssetId
    ) external onlyOwner nonReentrant {
        LiquidationParams memory params = LiquidationParams({
            user: user,
            collateralAsset: collateralAsset,
            debtAsset: debtAsset,
            debtToCover: debtToCover,
            receiveAToken: receiveAToken,
            collateralAssetId: collateralAssetId,
            debtAssetId: debtAssetId
        });

        bytes memory paramsBytes = abi.encode(params);

        uint256 actualDebtToCover = debtToCover;
        if (debtToCover == type(uint256).max) {
            uint256 assetDebt = _getUserAssetDebt(debtAsset, user);
            actualDebtToCover = assetDebt / 2;
        }

        address[] memory assets = new address[](1);
        assets[0] = debtAsset;
        uint256[] memory amounts = new uint256[](1);
        amounts[0] = actualDebtToCover;
        uint256[] memory modes = new uint256[](1);
        modes[0] = 0;

        IPool(POOL_ADDRESS).flashLoan(
            address(this),
            assets,
            amounts,
            modes,
            address(this),
            paramsBytes,
            0
        );
    }

    function executeOperation(
        address[] calldata assets,
        uint256[] calldata amounts,
        uint256[] calldata premiums,
        address initiator,
        bytes calldata params
    ) external override nonReentrant returns (bool) {
        require(msg.sender == POOL_ADDRESS, "Caller must be Aave Pool");
        require(initiator == address(this), "Invalid initiator");

        LiquidationParams memory p = abi.decode(params, (LiquidationParams));
        address debtAsset = assets[0];
        uint256 amount = amounts[0];
        uint256 premium = premiums[0];

        IERC20(debtAsset).safeApprove(POOL_ADDRESS, amount);
        _executeLiquidation(p, amount);

        uint256 collateralBalance = IERC20(p.collateralAsset).balanceOf(
            address(this)
        );
        uint256 debtAssetBalance = IERC20(debtAsset).balanceOf(address(this));
        if (p.collateralAsset != debtAsset && collateralBalance > 0) {
            debtAssetBalance += _swapCollateralToDebt(
                p.collateralAsset,
                debtAsset,
                collateralBalance
            );
        }

        uint256 totalRepay = amount + premium;
        require(debtAssetBalance >= totalRepay, "Insufficient funds");
        IERC20(debtAsset).safeApprove(POOL_ADDRESS, totalRepay);

        uint256 profit = debtAssetBalance - totalRepay;
        emit LiquidationExecuted(
            p.user,
            p.collateralAsset,
            debtAsset,
            amount,
            collateralBalance,
            profit
        );
        return true;
    }

    function _executeLiquidation(
        LiquidationParams memory p,
        uint256 debtToCover
    ) internal {
        bytes32 args1 = bytes32(
            (uint256(p.collateralAssetId) << 240) |
                (uint256(p.debtAssetId) << 224) |
                uint256(uint160(p.user))
        );
        bytes32 args2 = bytes32(
            (debtToCover << 128) | (p.receiveAToken ? 1 : 0)
        );
        IL2Pool(POOL_ADDRESS).liquidationCall(args1, args2);
    }

    function _getUserAssetDebt(
        address asset,
        address user
    ) internal view returns (uint256) {
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

    function _swapCollateralToDebt(
        address inToken,
        address outToken,
        uint256 amountIn
    ) internal returns (uint256 amountOut) {
        require(amountIn > 0, "Invalid swap amount");
        require(inToken != outToken, "Same token swap not allowed");

        IERC20(inToken).safeApprove(SWAP_ROUTER, amountIn);

        // Use configurable slippage protection instead of hardcoded value
        uint256 amountOutMin = (amountIn * (10000 - maxSlippage)) / 10000;

        // Use configurable deadline buffer to prevent manipulation
        uint256 deadline = block.timestamp + swapDeadlineBuffer;
        require(deadline > block.timestamp, "Invalid deadline");

        ISwapRouter.ExactInputSingleParams memory params = ISwapRouter
            .ExactInputSingleParams({
                tokenIn: inToken,
                tokenOut: outToken,
                fee: defaultSwapFee, // Use configurable fee tier
                recipient: address(this),
                deadline: deadline,
                amountIn: amountIn,
                amountOutMinimum: amountOutMin,
                sqrtPriceLimitX96: 0
            });

        amountOut = ISwapRouter(SWAP_ROUTER).exactInputSingle(params);
        require(amountOut >= amountOutMin, "Slippage tolerance exceeded");

        // Clean up approval
        IERC20(inToken).safeApprove(SWAP_ROUTER, 0);
    }

    function withdraw(
        address asset,
        uint256 amount,
        address to
    ) external onlyOwner {
        require(to != address(0), "Invalid recipient");
        uint256 bal = IERC20(asset).balanceOf(address(this));
        uint256 w = amount == 0 ? bal : amount;
        require(w <= bal && w > 0, "Nothing to withdraw");
        IERC20(asset).safeTransfer(to, w);
        emit ProfitWithdrawn(asset, w, to);
    }

    function withdrawETH(address payable to) external onlyOwner {
        require(to != address(0), "Invalid recipient");
        uint256 b = address(this).balance;
        require(b > 0, "No ETH");
        (bool s, ) = to.call{value: b}("");
        require(s, "ETH transfer failed");
        emit ProfitWithdrawn(address(0), b, to);
    }

    function setMinProfitThreshold(uint256 t) external onlyOwner {
        minProfitThreshold = t;
    }

    function setMaxSlippage(uint256 _maxSlippage) external onlyOwner {
        require(_maxSlippage <= 2000, "Slippage too high"); // Max 20%
        maxSlippage = _maxSlippage;
    }

    function setSwapDeadlineBuffer(uint256 _deadlineBuffer) external onlyOwner {
        require(
            _deadlineBuffer >= 60 && _deadlineBuffer <= 3600,
            "Invalid deadline buffer"
        ); // 1 min to 1 hour
        swapDeadlineBuffer = _deadlineBuffer;
    }

    function setDefaultSwapFee(uint24 _fee) external onlyOwner {
        require(
            _fee == 100 || _fee == 500 || _fee == 3000 || _fee == 10000,
            "Invalid fee tier"
        );
        defaultSwapFee = _fee;
    }

    function emergencyApprove(
        address token,
        address spender,
        uint256 amt
    ) external onlyOwner {
        IERC20(token).safeApprove(spender, amt);
    }

    function getPool() external view returns (address) {
        return POOL_ADDRESS;
    }

    function getSwapRouter() external view returns (address) {
        return SWAP_ROUTER;
    }

    function getAddressesProvider() external view returns (address) {
        return ADDRESSES_PROVIDER_ADDRESS;
    }

    function getDataProvider() external view returns (address) {
        return DATA_PROVIDER;
    }

    receive() external payable {}
}
