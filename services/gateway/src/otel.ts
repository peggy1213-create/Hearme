import { metrics, trace, type Span, type Tracer } from '@opentelemetry/api';

/**
 * OpenTelemetry wiring. The SDK is started only when `OTEL_EXPORTER_OTLP_ENDPOINT`
 * is set, so tests and local runs stay no-op. The instrument handles below come
 * from `@opentelemetry/api`, which returns no-op implementations until an SDK is
 * registered — so they are always safe to call.
 *
 * We record only ids, durations and error codes. No transcript or audio content
 * ever becomes a span attribute or metric label. See CLAUDE.md.
 */

const METER_NAME = 'hearme-gateway';
const TRACER_NAME = 'hearme-gateway';

const meter = metrics.getMeter(METER_NAME);

export const gatewayMetrics = {
  /** End-to-end latency from a segment's audio-end to it being sent, in ms. */
  segmentLatency: meter.createHistogram('hearme.segment.latency_ms', {
    description: 'Live segment latency (audio end -> sent) in ms',
    unit: 'ms',
  }),
  /** Vendor errors, tagged by vendor name and whether we failed over. */
  vendorErrors: meter.createCounter('hearme.vendor.errors', {
    description: 'STT vendor errors',
  }),
  /** Currently active live meetings. */
  activeMeetings: meter.createUpDownCounter('hearme.active_meetings', {
    description: 'Active live meetings',
  }),
};

export function getTracer(): Tracer {
  return trace.getTracer(TRACER_NAME);
}

export type { Span };

let started = false;

/**
 * Start the OTel Node SDK if an OTLP endpoint is configured. Returns a shutdown
 * function, or `undefined` if telemetry is disabled. Imports the SDK lazily so the
 * heavy dependencies are only loaded when actually exporting.
 */
export async function startTelemetry(
  env: NodeJS.ProcessEnv = process.env,
): Promise<(() => Promise<void>) | undefined> {
  if (started || !env.OTEL_EXPORTER_OTLP_ENDPOINT) return undefined;
  started = true;

  const { NodeSDK } = await import('@opentelemetry/sdk-node');
  const { OTLPTraceExporter } = await import('@opentelemetry/exporter-trace-otlp-http');
  const { OTLPMetricExporter } = await import('@opentelemetry/exporter-metrics-otlp-http');
  const { PeriodicExportingMetricReader } = await import('@opentelemetry/sdk-metrics');
  const { Resource } = await import('@opentelemetry/resources');
  const { ATTR_SERVICE_NAME } = await import('@opentelemetry/semantic-conventions');

  const sdk = new NodeSDK({
    resource: new Resource({
      [ATTR_SERVICE_NAME]: env.OTEL_SERVICE_NAME ?? 'hearme-gateway',
    }),
    traceExporter: new OTLPTraceExporter(),
    metricReader: new PeriodicExportingMetricReader({
      exporter: new OTLPMetricExporter(),
    }),
  });
  sdk.start();
  return () => sdk.shutdown();
}
