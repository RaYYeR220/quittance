export const REPO = "https://github.com/RaYYeR220/quittance";

export const DOCS = {
  addendum: `${REPO}/blob/main/docs/zip-0311-ironwood.md`,
  threatModel: `${REPO}/blob/main/docs/THREAT-MODEL.md`,
  proof: `${REPO}/blob/main/docs/PROOF.md`,
  licence: `${REPO}/blob/main/LICENSE`,
};

export function explorerTx(network: "main" | "test", txid: string): string {
  return network === "main" ? `https://mainnet.zcashexplorer.app/transactions/${txid}` : `https://testnet.zecblock.com/tx/${txid}`;
}
