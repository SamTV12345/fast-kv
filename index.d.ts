/* auto-generated by NAPI-RS */
/* eslint-disable */
export declare class KeyValueDB {
  constructor(filename: string)
  get(key: string): string | null
  set(key: string, value: string): void
  remove(key: string): void
  findKeys(key: string, notKey?: string | undefined | null): Array<string>
  destroy(): void
}

