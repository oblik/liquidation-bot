// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "../contracts-foundry/AaveLiquidator.sol";

contract DeployAaveLiquidator is Script {
    function run() external {
        uint256 key = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(key);

        address pool = 0xA238Dd80C259a72e81d7e4664a9801593F98d1c5;
        address provider = 0xe20fCBdBfFC4Dd138cE8b2E6FBb6CB49777ad64D;
        address router = 0x2626664c2603336E57B271c5C0b26F421741e481;

        new AaveLiquidator(pool, provider, router);

        vm.stopBroadcast();
    }
}