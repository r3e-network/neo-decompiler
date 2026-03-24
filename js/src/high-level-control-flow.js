import { createBranchHelpers } from "./high-level-branches.js";
import { createLoopHelpers } from "./high-level-loops.js";
import { createTryHelpers } from "./high-level-try.js";
import { rewriteForLoops } from "./high-level-control-flow-shared.js";

export function createControlFlowHelpers(runtime) {
  const branches = createBranchHelpers(runtime);
  const loops = createLoopHelpers(runtime);
  const tries = createTryHelpers(runtime);

  return {
    ...branches,
    ...loops,
    ...tries,
    rewriteForLoops,
  };
}
