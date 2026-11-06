type MarkdownNode = { children?: MarkdownNode[] };

/** Reject extreme AST depth before recursive plugins or DOM construction. */
export function remarkMaxNestingDepth({ maxDepth }: { maxDepth: number }) {
  return (tree: MarkdownNode) => {
    const pending = [{ node: tree, depth: 0 }];
    while (pending.length > 0) {
      const entry = pending.pop();
      if (!entry) break;
      const { node, depth } = entry;
      if (depth > maxDepth) {
        throw new Error("Markdown nesting limit exceeded");
      }
      for (const child of node.children ?? []) {
        pending.push({ node: child, depth: depth + 1 });
      }
    }
  };
}
