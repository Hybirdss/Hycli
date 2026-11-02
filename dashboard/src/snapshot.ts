export function snapshotCoordinator<T>(read: () => Promise<T>, publish: (value: T) => void, freshness: (fresh: boolean) => void) {
  // Reads begun before an accepted command cannot certify its resulting state.
  let epoch = 0;
  let sequence = 0;
  let settled = 0;
  let current: T | null = null;
  let fresh = false;
  const refresh = async (): Promise<T | null> => {
    const startedEpoch = epoch;
    const order = ++sequence;
    try {
      const value = await read();
      if (startedEpoch !== epoch || order < settled) return null;
      settled = order; current = value; fresh = true;
      publish(value); freshness(true);
      return value;
    } catch (error) {
      if (startedEpoch !== epoch || order < settled) return null;
      settled = order; fresh = false; freshness(false);
      throw error;
    }
  };
  return {
    refresh,
    freshSnapshot: () => fresh ? current : null,
    async act<R>(command: () => Promise<R>): Promise<R> {
      const receipt = await command();
      // The receipt is authoritative; synchronization has its own failure channel.
      epoch++; fresh = false; freshness(false);
      void refresh().catch(() => {});
      return receipt;
    },
  };
}
