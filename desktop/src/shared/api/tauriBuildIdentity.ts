import { invokeTauri } from "./tauri";

export type ForkBuildIdentity = {
  forkRevision: string;
  commitSha: string;
  baseTag: string;
};

type RawForkBuildIdentity = {
  fork_revision: string;
  commit_sha: string;
  base_tag: string;
};

/** Read immutable native build identity; upstream binaries return null. */
export async function getForkBuildIdentity(): Promise<ForkBuildIdentity | null> {
  const raw = await invokeTauri<RawForkBuildIdentity | null>(
    "get_fork_build_identity",
  );
  return raw
    ? {
        forkRevision: raw.fork_revision,
        commitSha: raw.commit_sha,
        baseTag: raw.base_tag,
      }
    : null;
}

/** Keep upstream version separate and show the full candidate SHA. */
export function forkBuildLabel(
  identity: ForkBuildIdentity | null,
): string | null {
  return identity
    ? `Fork ${identity.forkRevision} · ${identity.commitSha}`
    : null;
}
