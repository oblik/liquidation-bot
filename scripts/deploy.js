const { ethers } = require("hardhat");

async function main() {
  console.log("Deploying AaveLiquidator contract...");

  // Get the deployer account
  const [deployer] = await ethers.getSigners();
  console.log("Deploying with account:", deployer.address);
  console.log("Account balance:", ethers.utils.formatEther(await deployer.getBalance()));

  // Deploy the AaveLiquidator contract
  const AaveLiquidator = await ethers.getContractFactory("AaveLiquidator");
  const liquidator = await AaveLiquidator.deploy();

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
        constructorArguments: []
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
    timestamp: new Date().toISOString()
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