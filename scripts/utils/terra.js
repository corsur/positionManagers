import { LCDClient } from "@terra-money/terra.js";

// Internals.
const gasAdjustment = 1.5;

// Constants.
export const TERRA_MANAGER_MAINNET =
  "terra1ajkmy2c0g84seh66apv9x6xt6kd3ag80jmcvtz";
export const TERRA_MANAGER_TESTNET =
  "terra1pzmq3sacc2z3pk8el3rk0q584qtuuhnv4fwp8n";

export const mainnetTerra = new LCDClient({
  URL: "https://lcd.terra.dev",
  chainID: "columbus-5",
  gasPrices: gasPrices,
  gasAdjustment: gasAdjustment,
});

export const testnetTerra = new LCDClient({
  URL: "https://bombay-lcd.terra.dev",
  chainID: "bombay-12",
  gasPrices: gasPrices,
  gasAdjustment: gasAdjustment,
});