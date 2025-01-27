//! Common logic for sources that are HTTP clients.
//!
//! Specific HTTP client sources will:
//!   - Call build_url() to build the URL(s) to call.
//!   - Implement a specific context struct which:
//!       - Contains the data that source needs in order to process the HTTP responses into internal_events
//!       - Implements the HttpClient trait
//!   - Call call() supplying the generic inputs for calling and the source-specific
//!     context.

use self::super::vrl::VrlDeserializerConfig;
use crate::{
    http::{Auth, HttpClient},
    internal_events::{
        EndpointBytesReceived, HttpClientEventsReceived, HttpClientHttpError,
        HttpClientHttpResponseError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::TlsSettings,
    SourceSender,
};
use futures::TryFutureExt;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use http::{response::Parts, Uri};
use hyper::{Body, Request};
use std::collections::HashMap;
use std::time::Duration;
use vector_lib::json_size::JsonSize;
use vector_lib::shutdown::ShutdownSignal;
use vector_lib::{
    config::proxy::ProxyConfig,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    EstimatedJsonEncodedSizeOf,
};

/// Contains the inputs generic to any http client.
pub(crate) struct GenericPullerClientInputs {
    /// Array of URLs to call.
    pub urls: Vec<Uri>,

    /// VRL to build urls from the cursor
    pub url_vrl: VrlDeserializerConfig,
    /// VRL to parse the cursor from the response
    pub cursor_vrl: VrlDeserializerConfig,
    /// time range to pull data from
    pub time_range: Duration,
    /// Interval between calls.
    pub interval: Duration,
    /// Timeout for the HTTP request.
    pub timeout: Duration,
    /// Map of Header+Value to apply to HTTP request.
    pub headers: HashMap<String, Vec<String>>,
    /// Content type of the HTTP request, determined by the source.
    pub content_type: String,
    pub auth: Option<Auth>,
    pub tls: TlsSettings,
    pub proxy: ProxyConfig,
    pub shutdown: ShutdownSignal,
}
/// The default time range to pull data from
pub(crate) const fn default_time_range() -> Duration {
    Duration::from_secs(300)
}

/// The default interval to call the HTTP endpoint if none is configured.
pub(crate) const fn default_interval() -> Duration {
    Duration::from_secs(15)
}

/// The default timeout for the HTTP request if none is configured.
pub(crate) const fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

/// Builds the context, allowing the source-specific implementation to leverage data from the
/// config and the current HTTP request.
pub(crate) trait PullerClientBuilder {
    type Context: PullerClientContext;

    /// Called before the HTTP request is made to build out the context.
    fn build(&self, url: &Uri) -> Self::Context;
}

/// Methods that allow context-specific behavior during the scraping procedure.
pub(crate) trait PullerClientContext {
    /// Called after the HTTP request succeeds and returns the decoded/parsed Event array.
    fn on_response(&mut self, url: &Uri, header: &Parts, body: &Bytes) -> Option<Vec<Event>>;

    /// (Optional) Called if the HTTP response is not 200 ('OK').
    fn on_http_response_error(&self, _uri: &Uri, _header: &Parts) {}

    // This function can be defined to enrich events with additional HTTP
    // metadata. This function should be used rather than internal enrichment so
    // that accurate byte count metrics can be emitted.
    fn enrich_events(&mut self, _events: &mut Vec<Event>) {}
}

/// Builds a url for the HTTP requests.
pub(crate) fn build_url(uri: &Uri, query: &HashMap<String, Vec<String>>) -> Uri {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(query) = uri.query() {
        serializer.extend_pairs(url::form_urlencoded::parse(query.as_bytes()));
    };
    for (k, l) in query {
        for v in l {
            serializer.append_pair(k, v);
        }
    }
    let mut builder = Uri::builder();
    if let Some(scheme) = uri.scheme() {
        builder = builder.scheme(scheme.clone());
    };
    if let Some(authority) = uri.authority() {
        builder = builder.authority(authority.clone());
    };
    builder = builder.path_and_query(match serializer.finish() {
        query if !query.is_empty() => format!("{}?{}", uri.path(), query),
        _ => uri.path().to_string(),
    });
    builder
        .build()
        .expect("Failed to build URI from parsed arguments")
}

/// Warns if the scrape timeout is greater than the scrape interval.
pub(crate) fn warn_if_interval_too_low(timeout: Duration, interval: Duration) {
    if timeout > interval {
        warn!(
            interval_secs = %interval.as_secs_f64(),
            timeout_secs = %timeout.as_secs_f64(),
            message = "Having a scrape timeout that exceeds the scrape interval can lead to excessive resource consumption.",
        );
    }
}

/// Skip if the timestamp is in the future
pub(crate) fn skip_if_timestamp_is_future(timestamp: DateTime<Utc>) -> bool {
    let now = Utc::now();
    timestamp > now
}

/// Calls one or more urls at an interval.
///   - The HTTP request is built per the options in provided generic inputs.
///   - The HTTP response is decoded/parsed into events by the specific context.
///   - The events are then sent to the output stream.
pub(crate) async fn call<
    B: PullerClientBuilder<Context = C> + Send + Clone,
    C: PullerClientContext + Send,
>(
    inputs: GenericPullerClientInputs,
    context_builder: B,
    mut out: SourceSender,
    http_method: HttpMethod,
) -> Result<(), ()> {
    if skip_if_timestamp_is_future(Utc::now()) {
        println!("skipping");
        return Ok(());
    }

    let time_range = inputs.time_range;
    println!("time_range: {:?}", time_range);

    let client =
        HttpClient::new(inputs.tls.clone(), &inputs.proxy).expect("Building HTTP client failed");

    loop {
        tokio::select! {
            _ = inputs.shutdown.clone() => break,

            _ = async {} => {
                for url in &inputs.urls {
                    let url_vrl = inputs.url_vrl.build();
                    println!("url_vrl: {:?}", url_vrl);
                    let endpoint = url.to_string();
                    let cursor_vrl = inputs.cursor_vrl.build();
                    let mut context = context_builder.clone().build(url);

                    let mut builder = match http_method {
                        HttpMethod::Head => Request::head(url),
                        HttpMethod::Get => Request::get(url),
                        HttpMethod::Post => Request::post(url),
                        HttpMethod::Put => Request::put(url),
                        HttpMethod::Patch => Request::patch(url),
                        HttpMethod::Delete => Request::delete(url),
                        HttpMethod::Options => Request::options(url),
                    };

                    // Add headers
                    for (header, values) in &inputs.headers {
                        for value in values {
                            builder = builder.header(header, value);
                        }
                    }

                    if !inputs.headers.contains_key(http::header::ACCEPT.as_str()) {
                        builder = builder.header(http::header::ACCEPT, &inputs.content_type);
                    }

                    let mut request = builder.body(Body::empty()).expect("error creating request");

                    if let Some(auth) = &inputs.auth {
                        auth.apply(&mut request);
                    }

                    // Make the request with timeout
                    let response = match tokio::time::timeout(inputs.timeout, client.send(request)).await {
                        Ok(Ok(response)) => response,
                        Ok(Err(error)) => {
                            emit!(HttpClientHttpError {
                                error: Box::new(error),
                                url: url.to_string()
                            });
                            continue;
                        }
                        Err(_) => {
                            emit!(HttpClientHttpError {
                                error: format!("Timeout error: request exceeded {}s", inputs.timeout.as_secs_f64()).into(),
                                url: url.to_string()
                            });
                            continue;
                        }
                    };

                    // Process response
                    let (header, body) = response.into_parts();
                    let body = match hyper::body::to_bytes(body).await {
                        Ok(body) => body,
                        Err(e) => {
                            emit!(HttpClientHttpError {
                                error: e.into(),
                                url: url.to_string()
                            });
                            continue;
                        }
                    };

                    emit!(EndpointBytesReceived {
                        byte_size: body.len(),
                        protocol: "http",
                        endpoint: endpoint.as_str(),
                    });

                    if header.status == hyper::StatusCode::OK {
                        let _cursor_map = cursor_vrl.as_ref().unwrap().parse_cursor(body.clone());
                        println!("cursor_map: {:?}", _cursor_map);

                        if let Some(mut events) = context.on_response(url, &header, &body) {
                            let byte_size = if events.is_empty() {
                                JsonSize::zero()
                            } else {
                                events.estimated_json_encoded_size_of()
                            };

                            emit!(HttpClientEventsReceived {
                                byte_size,
                                count: events.len(),
                                url: url.to_string()
                            });

                            context.enrich_events(&mut events);

                            let receiver = BatchNotifier::maybe_apply_to(true, &mut events);
                            let count = events.len();
                            let _ = out.send_batch(events)
                                .map_err(|_| {
                                    // can only fail if receiving end disconnected, so we are shutting down,
                                    // probably not gracefully.
                                    emit!(StreamClosedError { count });
                                })
                                .and_then(|_| handle_batch_status(receiver))
                                .await;
                        }
                    } else {
                        context.on_http_response_error(url, &header);
                        emit!(HttpClientHttpResponseError {
                            code: header.status,
                            url: url.to_string(),
                        });
                    }
                }
                // Wait for the configured interval before starting the next iteration
                tokio::time::sleep(inputs.interval).await;
            }
        }
    }

    Ok(())
}

async fn handle_batch_status(
    receiver: Option<BatchStatusReceiver>,
) -> Result<(), ()> {
    match receiver {
        None => Ok(()),
        Some(receiver) => match receiver.await {
            BatchStatus::Delivered => Ok(()),
            BatchStatus::Errored => Err(()),
            BatchStatus::Rejected => Err(()),
        },
    }
}
