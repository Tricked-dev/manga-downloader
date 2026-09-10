declare module "sonda/sveltekit" {
  import type { UserOptions } from "sonda";
  import type { PluginOption } from "vite";

  export default function Sonda(options?: UserOptions): PluginOption;
}
