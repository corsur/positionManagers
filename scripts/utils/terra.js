import { LCDClient } from "@terra-money/terra.js";

// Internals.
const gasAdjustment = 1.5;
const gasPrices = {
  uusd: 0.15,
};

// Constants.
export const TERRA_MANAGER_MAINNET =
  "terra1ajkmy2c0g84seh66apv9x6xt6kd3ag80jmcvtz";
export const TERRA_MANAGER_TESTNET =
  "terra1pzmq3sacc2z3pk8el3rk0q584qtuuhnv4fwp8n";
export const MIRROR_ORACLE_MAINNET =
  "terra1t6xe0txzywdg85n6k8c960cuwgh6l8esw6lau9";
export const MIRROR_ORACLE_TESTNET =
  "terra1uvxhec74deupp47enh7z5pk55f3cvcz8nj4ww9";
export const TERRA_CHAIN_ID = 3;
export const DELTA_NEUTRAL_STRATEGY_ID = "0";

export const mainnetTerra = new LCDClient({
  // URL: "https://lcd.terra.dev",
  URL: "https://broken-aged-feather.terra-mainnet.quiknode.pro/6536ded4ddea43ff5b4b4318ec4cdf90f2ce4aee/",
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
