import 'dart:async';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

/// Longest wait for response headers or the next body chunk. A slow link that
/// keeps delivering bytes finishes; one that stops is aborted.
const defaultStallTimeout = Duration(seconds: 20);

/// Longest a fetch may take even while bytes keep arriving, so a server that
/// trickles bytes cannot hold a connection indefinitely.
const defaultMaxFetchDuration = Duration(minutes: 5);

/// An observed redirect response, before its body is discarded.
typedef BoundedRedirect = ({int statusCode, Map<String, String> headers});

/// A GET response read by [boundedGet]. [bytes] is empty unless the status
/// was 200: an error body is never read.
class BoundedResponse {
  const BoundedResponse(
    this.statusCode,
    this.headers,
    this.bytes, {
    this.url,
    this.redirectTarget,
    this.redirects = const [],
  });

  final int statusCode;
  final Map<String, String> headers;
  final Uint8List bytes;

  /// Response URL reported by the client, if available. Some clients supply
  /// a relative redirect target without its base URL.
  final Uri? url;

  /// Last requested URL when [boundedGet] followed redirects explicitly.
  final Uri? redirectTarget;

  /// Intermediate responses in the order they were followed.
  final List<BoundedRedirect> redirects;
}

/// The 200 body of a [boundedGet] exceeded its `maxBytes`.
class ResponseTooLargeException implements Exception {
  const ResponseTooLargeException(this.uri);

  final Uri uri;

  @override
  String toString() => 'ResponseTooLargeException: $uri';
}

/// The caller's `cancel` future completed before [boundedGet] finished.
class FetchCancelledException implements Exception {
  const FetchCancelledException(this.uri);

  final Uri uri;

  @override
  String toString() => 'FetchCancelledException: $uri';
}

/// GETs [uri] without closing [client], aborting the request when no headers
/// or body bytes arrive for [stallTimeout], when it runs past
/// [maxFetchDuration], or when [cancel] completes. A 200 body larger than
/// [maxBytes], declared or streamed, throws [ResponseTooLargeException]; other statuses return
/// without reading the body. A stall or deadline throws the client's abort
/// error ([http.RequestAbortedException]); a cancel throws
/// [FetchCancelledException].
///
/// When [sendRedirect] is supplied, redirects are observed and followed one
/// hop at a time with that sender, within this same fetch deadline. Original
/// request headers are dropped at the first change of origin.
Future<BoundedResponse> boundedGet(
  http.Client client,
  Uri uri, {
  required int maxBytes,
  Map<String, String> headers = const {},
  bool followRedirects = true,
  Future<http.StreamedResponse> Function(http.BaseRequest)? sendRedirect,
  Duration stallTimeout = defaultStallTimeout,
  Duration maxFetchDuration = defaultMaxFetchDuration,
  Future<void>? cancel,
}) async {
  final stop = Completer<void>();
  var cancelled = false;
  Timer? stall;
  void restartStallTimer() {
    stall?.cancel();
    stall = Timer(stallTimeout, () {
      if (!stop.isCompleted) stop.complete();
    });
  }

  restartStallTimer();
  final deadline = Timer(maxFetchDuration, () {
    if (!stop.isCompleted) stop.complete();
  });
  unawaited(
    cancel?.then((_) {
      if (stop.isCompleted) return;
      cancelled = true;
      stop.complete();
    }),
  );
  StreamIterator<List<int>>? body;
  try {
    var target = uri;
    var forwardedHeaders = headers;
    final redirects = <BoundedRedirect>[];
    late http.StreamedResponse streamed;
    for (;;) {
      final request =
          http.AbortableRequest('GET', target, abortTrigger: stop.future)
            ..headers.addAll(forwardedHeaders)
            ..followRedirects = followRedirects && sendRedirect == null;
      streamed = await (redirects.isEmpty ? client.send : sendRedirect!)(
        request,
      );
      restartStallTimer();
      final next = followRedirects && sendRedirect != null
          ? _redirectTarget(target, streamed)
          : null;
      if (next == null) break;
      // The redirect body is irrelevant, including errors during disposal.
      streamed.stream
          .listen(null, onError: (Object _, StackTrace _) {})
          .cancel()
          .ignore();
      if (stop.isCompleted) throw http.RequestAbortedException(uri);
      if (redirects.length >= request.maxRedirects) {
        throw http.ClientException('Redirect limit exceeded', uri);
      }
      redirects.add((
        statusCode: streamed.statusCode,
        headers: streamed.headers,
      ));
      if (next.origin != target.origin) forwardedHeaders = const {};
      target = next;
    }
    if (streamed.statusCode == 200 &&
        (streamed.contentLength ?? 0) > maxBytes) {
      unawaited(streamed.stream.listen(null).cancel());
      throw ResponseTooLargeException(uri);
    }
    final iterator = body = StreamIterator(streamed.stream);
    final bytes = BytesBuilder(copy: false);
    // An error body is never shown, and reading a slow one could hold the
    // caller for the whole fetch cap. Leave terminal error bodies unread.
    while (streamed.statusCode == 200 && await iterator.moveNext()) {
      restartStallTimer();
      final chunk = iterator.current;
      if (chunk.length > maxBytes - bytes.length) {
        throw ResponseTooLargeException(uri);
      }
      bytes.add(chunk);
    }
    return BoundedResponse(
      streamed.statusCode,
      streamed.headers,
      bytes.takeBytes(),
      url: switch (streamed) {
        http.BaseResponseWithUrl(:final url) => url,
        _ => null,
      },
      redirectTarget: redirects.isEmpty ? null : target,
      redirects: redirects,
    );
  } catch (_) {
    if (cancelled) throw FetchCancelledException(uri);
    rethrow;
  } finally {
    stall?.cancel();
    deadline.cancel();
    if (!stop.isCompleted) stop.complete();
    unawaited(body?.cancel());
  }
}

Uri? _redirectTarget(Uri uri, http.StreamedResponse response) {
  switch (response.statusCode) {
    case 301 || 302 || 303 || 307 || 308:
      final location = response.headers['location'];
      if (location == null || location.isEmpty) return null;
      final parsed = Uri.tryParse(location);
      if (parsed == null) return null;
      final target = uri.resolveUri(parsed);
      return (target.scheme == 'http' || target.scheme == 'https') &&
              target.host.isNotEmpty &&
              target.userInfo.isEmpty
          ? target
          : null;
    default:
      return null;
  }
}
