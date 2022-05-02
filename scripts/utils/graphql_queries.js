import JSON5 from "json5";
import { TERRA_CHAIN_ID } from "./terra.js";

export function wasmQuery(tag, addr, query) {
  return `
      ${tag}: wasm {
          contractQuery(
            contractAddress: "${addr}"
            query: ${JSON5.stringify(query, { quote: '"' })}
          )
      }`;
}

export function getPositionInfoQueries(position_ids, addr) {
  return (
    "{" +
    position_ids
      .map((position_id) => {
        if (
          position_id == 905 ||
          position_id == 1104 ||
          position_id == 1953 ||
          position_id == 2613
        ) {
          return;
        }
        return wasmQuery(`q${position_id}`, addr, {
          batch_get_position_info: {
            positions: [
              {
                chain_id: TERRA_CHAIN_ID,
                position_id: position_id.toString(),
              },
            ],
          },
        });
      })
      .join(",") +
    "}"
  );
}

export function getMAssetQuoteQueries(mirror_oracle_addr, addrs) {
  return (
    "{" +
    addrs
      .map((addr) =>
        wasmQuery(`${addr}`, mirror_oracle_addr, {
          price: {
            asset_token: addr,
          },
        })
      )
      .join(",") +
    "}"
  );
}

export function getMAssetRequiredCRQueries(mirror_mint_addr, addrs) {
  return (
    "{" +
    addrs
      .map((addr) =>
        wasmQuery(`${addr}`, mirror_mint_addr, {
          asset_config: {
            asset_token: addr,
          },
        })
      )
      .join(",") +
    "}"
  );
}
