const { ethers } = require("hardhat");

async function main() {
  console.log("Deploying AaveLiquidator contract...");

  // Get the deployer account
  const [deployer] = await ethers.getSigners();
  console.log("Deploying with account:", deployer.address);
  console.log("Account balance:", ethers.utils.formatEther(await deployer.getBalance()));

  // Network-specific addresses
  const networkAddresses = {
    "base": {
      poolAddress: ethers.utils.getAddress("0xA238Dd80C259a72e81d7e4664a9801593F98d1c5"),
      addressesProvider: ethers.utils.getAddress("0x2f39d218133AFaB8F2B819B1066c7E434Ad94E9e"),
      swapRouter: ethers.utils.getAddress("0x2626664c2603336E57B271c5C0b26F421741e481")
    },
    "base-sepolia": {
      poolAddress: ethers.utils.getAddress("0xA37D7E3d3CaD89b44f9a08A96fE01a9F39Bd7794"),
      addressesProvider: ethers.utils.getAddress("0x0d8176c0e8965f2730c4c1aa5aae816fe4b7a802"),
      swapRouter: ethers.utils.getAddress("0x8357227d4edd91c4f85615c9cc5761899cd4b068")
    }
  };

  // Get addresses for current network
  const currentNetwork = network.name;
  const addresses = networkAddresses[currentNetwork];

  if (!addresses) {
    throw new Error(`Unsupported network: ${currentNetwork}. Supported networks: ${Object.keys(networkAddresses).join(", ")}`);
  }

  console.log(`Using addresses for network: ${currentNetwork}`);
  console.log(`Pool Address: ${addresses.poolAddress}`);
  console.log(`AddressesProvider: ${addresses.addressesProvider}`);
  console.log(`SwapRouter: ${addresses.swapRouter}`);

  // Deploy the AaveLiquidator contract with network-specific addresses
  const AaveLiquidator = await ethers.getContractFactory("AaveLiquidator");
  const liquidator = await AaveLiquidator.deploy(
    addresses.poolAddress,
    addresses.addressesProvider,
    addresses.swapRouter
  );

  await liquidator.deployed();

  console.log("AaveLiquidator deployed to:", liquidator.address);

  // Verify contract on Basescan (if not on local network)
  if (network.name !== "hardhat" && network.name !== "localhost") {
    console.log("Waiting for block confirmations...");
    await liquidator.deployTransaction.wait(5);

    console.log("Verifying contract on Basescan...");
    try {
      await hre.run("verify:verify", {
        address: liquidator.address,
        constructorArguments: [
          addresses.poolAddress,
          addresses.addressesProvider,
          addresses.swapRouter
        ]
      });
      console.log("Contract verified successfully");
    } catch (error) {
      console.log("Verification failed:", error.message);
    }
  }

  // Save deployment info
  const deploymentInfo = {
    network: network.name,
    contractAddress: liquidator.address,
    deployerAddress: deployer.address,
    deploymentTx: liquidator.deployTransaction.hash,
    blockNumber: liquidator.deployTransaction.blockNumber,
    timestamp: new Date().toISOString(),
    constructorArguments: {
      poolAddress: addresses.poolAddress,
      addressesProvider: addresses.addressesProvider,
      swapRouter: addresses.swapRouter
    }
  };

  console.log("\nDeployment Info:");
  console.log(JSON.stringify(deploymentInfo, null, 2));

  // Write deployment info to file
  const fs = require('fs');
  const deploymentPath = `./deployments/${network.name}.json`;

  // Create deployments directory if it doesn't exist
  if (!fs.existsSync('./deployments')) {
    fs.mkdirSync('./deployments');
  }

  fs.writeFileSync(deploymentPath, JSON.stringify(deploymentInfo, null, 2));
  console.log(`\nDeployment info saved to: ${deploymentPath}`);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  }); 