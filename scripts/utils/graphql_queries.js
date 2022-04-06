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
      .map((position_id) =>
        wasmQuery(`q${position_id}`, addr, {
          batch_get_position_info: {
            positions: [
              {
                chain_id: TERRA_CHAIN_ID,
                position_id: position_id.toString(),
              },
            ],
          },
        })
      )
      .join(",") +
    "}"
  );
}
