export interface MoleculerRsClientOptions {
  /** Base URL of the Laboratory Agent. Default: "http://localhost:3210" */
  endpoint?: string;
  /** Bearer token, if the broker was started with one. */
  token?: string | null;
  /** Request timeout in milliseconds. Default: 10000 */
  timeout?: number;
}

export class MoleculerRsError extends Error {
  status: number | null;
  body: unknown;
  constructor(message: string, status: number | null, body: unknown);
}

export class MoleculerRsClient {
  readonly endpoint: string;
  readonly token: string | null;
  readonly timeout: number;

  constructor(options?: MoleculerRsClientOptions);

  // Broker info
  health(): Promise<{ ok: boolean; [key: string]: unknown }>;
  info(): Promise<Record<string, unknown>>;

  // Registry
  services(): Promise<unknown[]>;
  actions(): Promise<unknown[]>;
  topology(): Promise<unknown[]>;

  // Observability
  metrics(): Promise<Record<string, unknown>>;
  traces(): Promise<unknown[]>;
  logs(): Promise<unknown[]>;
  metricsPrometheus(): Promise<string>;

  // Circuit breakers & cache
  circuitBreakers(): Promise<Record<string, unknown>>;
  cache(): Promise<Record<string, unknown>>;

  // Channels
  channels(): Promise<Record<string, unknown>>;
  sendToChannel(channel: string, payload: Record<string, unknown>): Promise<unknown>;
  retryDlq(id: string): Promise<unknown>;

  // Action calling
  call(action: string, params?: Record<string, unknown>): Promise<unknown>;
  emit(event: string, payload?: Record<string, unknown>): Promise<unknown>;
}
