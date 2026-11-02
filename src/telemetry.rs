//! Opt-in library traces. No paths, track metadata, or error details are recorded.

use opentelemetry::trace::{Span, Status, Tracer, TracerProvider};
use opentelemetry_otlp::ExporterBuildError;
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    resource::EnvResourceDetector,
    trace::{SdkTracer, SdkTracerProvider},
};

/// A closed set prevents callers from putting private data in span names.
#[derive(Clone, Copy)]
pub enum LibraryOperation {
    Load,
    AddFolder,
    Rescan,
}

impl LibraryOperation {
    fn name(self) -> &'static str {
        match self {
            Self::Load => "library.load",
            Self::AddFolder => "library.addFolder",
            Self::Rescan => "library.rescan",
        }
    }
}

pub struct Telemetry {
    provider: Option<SdkTracerProvider>,
    tracer: Option<SdkTracer>,
}

impl Telemetry {
    /// Initialize once on a normal OS thread, not an async runtime thread.
    /// The exporter reads standard OTLP endpoint, protocol, and header variables.
    /// An absent or empty base endpoint disables telemetry, even if a signal-specific
    /// endpoint is set. Export runs on the SDK's dedicated batch worker thread.
    pub fn init() -> Result<Self, ExporterBuildError> {
        if !std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .is_ok_and(|endpoint| !endpoint.trim().is_empty())
        {
            return Ok(Self {
                provider: None,
                tracer: None,
            });
        }

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build()?;
        // Honor explicitly supplied resource attributes, but do not detect the
        // host, process, executable path, or operating-system user.
        let resource = Resource::builder_empty()
            .with_detector(Box::new(EnvResourceDetector::new()))
            .with_service_name("local-music-desktop")
            .build();
        let provider = SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("local-music-library");
        Ok(Self {
            provider: Some(provider),
            tracer: Some(tracer),
        })
    }

    /// Run synchronous library work and return its result unchanged. In particular,
    /// the error needs no Display/Debug bound and is never serialized or recorded.
    pub fn trace_library_operation<T, E>(
        &self,
        operation: LibraryOperation,
        run: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let Some(tracer) = &self.tracer else {
            return run();
        };
        let mut span = tracer.start(operation.name());
        let result = run();
        if result.is_err() {
            span.set_status(Status::error(""));
        }
        span.end();
        result
    }

    /// Flush queued spans and stop the worker after library work has finished.
    /// Call before process exit; do not shut down after each operation.
    pub fn shutdown(&self) -> OTelSdkResult {
        match &self.provider {
            Some(provider) => provider.shutdown(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_sdk::trace::{SpanData, SpanExporter};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Default)]
    struct Capture(Arc<Mutex<Vec<SpanData>>>);

    impl SpanExporter for Capture {
        async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
            self.0.lock().unwrap().extend(batch);
            Ok(())
        }
    }

    #[test]
    fn operations_record_only_names_duration_and_failure_status() {
        let capture = Capture::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(capture.clone())
            .build();
        let telemetry = Telemetry {
            tracer: Some(provider.tracer("local-music-library")),
            provider: Some(provider),
        };

        let value: Result<_, ()> =
            telemetry.trace_library_operation(LibraryOperation::Load, || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                Ok(42)
            });
        assert_eq!(value, Ok(42));
        // This error deliberately has no Display or Debug implementation.
        struct PrivateError;
        let failure: Result<(), _> =
            telemetry.trace_library_operation(LibraryOperation::AddFolder, || Err(PrivateError));
        assert!(failure.is_err());
        let _: Result<(), ()> =
            telemetry.trace_library_operation(LibraryOperation::Rescan, || Ok(()));
        telemetry.shutdown().unwrap();

        let spans = capture.0.lock().unwrap();
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].name, "library.load");
        assert!(spans[0].end_time > spans[0].start_time);
        assert_eq!(spans[0].status, Status::Unset);
        assert_eq!(spans[1].name, "library.addFolder");
        assert_eq!(spans[1].status, Status::error(""));
        assert_eq!(spans[2].name, "library.rescan");
        for span in spans.iter() {
            assert!(span.attributes.is_empty());
            assert!(span.events.is_empty());
            assert!(span.links.is_empty());
        }
    }

    #[test]
    fn disabled_telemetry_returns_the_operation_result() {
        let telemetry = Telemetry {
            provider: None,
            tracer: None,
        };
        assert_eq!(
            telemetry.trace_library_operation(LibraryOperation::Load, || Err::<(), _>("private")),
            Err("private")
        );
        telemetry.shutdown().unwrap();
    }
}
