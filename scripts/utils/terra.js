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
  "terra1t5k2c2p2kf5as247egz53rj8g8g2x4jw9qte9a";
export const MIRROR_ORACLE_TESTNET =
  "terra1sdr3rya4h039f4htfm42q44x3dlaxra7hc7p8e";
export const MIRROR_MINT_MAINNET =
  "terra1wfz7h3aqf4cjmjcvc6s8lxdhh7k30nkczyf0mj";
export const MIRROR_MINT_TESTNET =
  "terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w";
export const TERRA_CHAIN_ID = 3;
export const CR_SAFETY_MARGIN = 0.3;
export const DELTA_NEUTRAL_STRATEGY_ID = "0";
export const mAssetMap = {
  terra1vxtwu4ehgzz77mnfwrntyrmgl64qjs75mpwqaz: "mAAPL",
  terra14y5affaarufk3uscy2vr6pe6w6zqf2wpjzn5sh: "mTSLA",
  terra1jsxngqasf2zynj5kyh0tgq9mj3zksa5gk35j4k: "mNFLX",
  terra1dk3g53js3034x4v5c3vavhj2738une880yu6kx: "mETH",
  terra1csk6tc7pdmpr782w527hwhez6gfv632tyf72cp: "mQQQ",
  terra1cc3enj9qgchlrj34cnzhwuclc4vl2z3jl7tkqg: "mTWTR",
  terra1227ppwxxj3jxz8cfgq00jgnxqcny7ryenvkwj6: "mMSFT",
  terra165nd2qmrtszehcfrntlplzern7zl4ahtlhd5t2: "mAMZN",
  terra1w7zgkcyt7y4zpct9dw8mw362ywvdlydnum2awa: "mBABA",
  terra10h7ry7apm55h4ez502dqdv9gr53juu85nkd4aq: "mIAU",
  terra1kscs6uhrqwy6rx5kuw5lwpuqvm3t6j2d6uf2lp: "mSLV",
  terra1lvmx8fsagy70tv0fhmfzdw9h6s3sy4prz38ugf: "mUSO",
  terra19cmt6vzvhnnnfsmccaaxzy2uaj06zjktu6yzjx: "mVIXY",
  terra1mqsjugsugfprn3cvgxsrr8akkvdxv2pzc74us7: "mFB",
  terra1h8arz2k547uvmpxctuwush3jzc8fun4s96qgwt: "mGOOGL",
  terra18wayjpyq28gd970qzgjfmsjj7dmgdk039duhph: "mCOIN",
  terra18yqdfzfhnguerz9du5mnvxsh5kxlknqhcxzjfr: "mHOOD",
  terra1qqfx5jph0rsmkur2zgzyqnfucra45rtjae5vh6: "mARKK",
  terra1l5lrxtwd98ylfy09fn866au6dp76gu8ywnudls: "mGLXY",
  terra1u43zu5amjlsgty5j64445fr9yglhm53m576ugh: "mSQ",
  terra1g4x2pzmkc9z3mseewxf758rllg08z3797xly0n: "mABNB",
  terra1aa00lpfexyycedfg5k2p60l9djcmw0ue5l8fhc: "mSPY",
  terra19ya4jpvjvvtggepvmmj6ftmwly3p7way0tt08r: "mDOT",
  terra18ej5nsuu867fkx4tuy2aglpvqjrkcrjjslap3z: "mAMD",
  terra137drsu8gce5thf6jr5mxlfghw36rpljt3zj73v: "mGS",
  terra1qsnj5gvq8rgs7yws8x5u02gwd5wvtu4tks0hjm: "mKO",
  terra1rh2907984nudl7vh56qjdtvv7947z4dujj92sx: "mPYPL",
  terra1246zy658dfgtausf0c4a6ly8sc2e285q4kxqga: "mSBUX",
  terra1ptdxmj3xmmljzx02nr4auwfuelmj0cnkh8egs2: "mJNJ",
  terra1rhhvx8nzfrx5fufkuft06q5marfkucdqwq5sjw: "mBTC",
  terra1drsjzvzej4h4qlehcfwclxg4w5l3h5tuvd3jd8: "mNVDA",
  terra1dj2cj02zak0nvwy3uj9r9dhhxhdwxnw6psse6p: "mNIO",
  terra149755r3y0rve30e209awkhn5cxgkn5c8ju9pm5: "mDIS",
  terra17ana8hvzea0q7w367dm0dw48sxwql39qekpt7g: "mNKE",
};
export const DYNAMODB_BATCH_WRITE_ITEM_LIMIT = 25;

export const mainnetTerraController = new LCDClient({
  // URL: "https://lcd.terra.dev",
  URL: "https://columbus-5--lcd--full.datahub.figment.io/apikey/df60f3698c291c5e528c2e603931c73e/",
  chainID: "columbus-5",
  gasPrices: gasPrices,
  gasAdjustment: gasAdjustment,
});

export const mainnetTerraData = new LCDClient({
  // URL: "https://lcd.terra.dev",
  URL: "https://columbus-5--lcd--full.datahub.figment.io/apikey/df60f3698c291c5e528c2e603931c73e/",
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

export const delay = (ms) => new Promise((res) => setTimeout(res, ms));