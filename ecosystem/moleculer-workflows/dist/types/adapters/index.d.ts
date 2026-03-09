import BaseAdapter, { BaseDefaultOptions } from "./base.ts";
import RedisAdapter, { RedisAdapterOptions } from "./redis.ts";
declare const Adapters: {
    Base: typeof BaseAdapter;
    Redis: typeof RedisAdapter;
};
export type ResolvableAdapterType = keyof typeof Adapters | string | {
    type: keyof typeof Adapters | typeof BaseAdapter;
    options: BaseDefaultOptions | RedisAdapterOptions;
};
/**
 * Resolve adapter by name
 *
 * @param opt
 */
declare function resolve(opt?: ResolvableAdapterType): BaseAdapter;
/**
 * Register a new Channel Adapter
 *
 * @param name
 * @param value
 */
declare function register(name: string, value: BaseAdapter): void;
declare const _default: {
    Base: typeof BaseAdapter;
    Redis: typeof RedisAdapter;
} & {
    resolve: typeof resolve;
    register: typeof register;
};
export default _default;
//# sourceMappingURL=index.d.ts.map