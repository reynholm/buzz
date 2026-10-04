/** A saved definition survives instance failure; recovery uses its existing Start. */
export function savedDefinitionRecovery(name: string, cause: unknown): string {
  const reason =
    cause instanceof Error ? cause.message : "Instance creation failed.";
  return `${name} was saved. ${reason} Use Start on the saved agent in My Agents after synchronization; do not create it again.`;
}
