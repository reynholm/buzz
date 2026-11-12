part of '../channel_detail_page_test.dart';

Channel _soloThreadChannel({
  bool solo = true,
  bool member = true,
  bool archived = false,
}) => Channel(
  id: _channelId,
  name: solo ? 'Solo' : 'Team',
  channelType: 'stream',
  visibility: 'private',
  labels: solo ? const ['buzz.solo'] : const [],
  description: '',
  createdBy: 'self',
  createdAt: DateTime(2026),
  memberCount: 2,
  isMember: member,
  archivedAt: archived ? DateTime(2026) : null,
);

Future<void> _waitForSoloImages(
  WidgetTester tester,
  Finder scope, {
  int minimum = 1,
}) async {
  final decoded = find.descendant(
    of: scope,
    matching: find.byWidgetPredicate(
      (widget) => widget is RawImage && widget.image != null,
    ),
  );
  for (var i = 0; i < 500 && decoded.evaluate().length < minimum; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump();
  }
  expect(decoded.evaluate().length, greaterThanOrEqualTo(minimum));
}

void soloThreadTests() {
  for (final solo in [false, true]) {
    for (final member in [false, true]) {
      for (final archived in [false, true]) {
        testWidgets(
          'stream thread composer respects membership and archive: solo=$solo member=$member archived=$archived',
          (tester) async {
            final channel = _soloThreadChannel(
              solo: solo,
              member: member,
              archived: archived,
            );
            final events = [
              _textMsg(
                id: threadIdRoot,
                pubkey: 'agent',
                content: 'Thread head',
              ),
            ];
            final messages = formatTimeline(events);
            await tester.pumpWidget(
              _buildTestable(
                channel: channel,
                messages: events,
                signingPubkey: 'self',
                threadReplies: {threadIdRoot: const []},
                home: ThreadDetailPage(
                  threadHead: messages.single,
                  allMessages: messages,
                  channelId: _channelId,
                  currentPubkey: 'self',
                  isMember: member,
                  isArchived: archived,
                ),
              ),
            );
            await tester.pumpAndSettle();
            expect(find.text('Thread head'), findsOneWidget);
            expect(
              find.byType(ComposeBar),
              member && !archived ? findsOneWidget : findsNothing,
            );
            expect(tester.takeException(), isNull);
          },
        );
      }
    }
  }

  for (final initialTarget in [false, true]) {
    testWidgets(
      'Solo stream nested reply preserves outer root and draft after reconnect: initialTarget=$initialTarget',
      (tester) async {
        final channel = _soloThreadChannel();
        final sender = _RecordingOrdinarySender();
        final topLevelSender = _RecordingOrdinarySender();
        late _ReconnectingRelaySession relay;
        final events = [
          _textMsg(id: threadIdRoot, pubkey: 'self', content: 'Outer question'),
          _textMsg(
            id: threadIdA,
            pubkey: 'agent',
            content: 'Nested branch',
            createdAt: 1001,
            extraTags: const [
              ['e', threadIdRoot, '', 'root'],
              ['e', threadIdRoot, '', 'reply'],
            ],
          ),
        ];
        final messages = formatTimeline(events);
        Widget build() {
          relay = _ReconnectingRelaySession(
            initialStatus: SessionStatus.connected,
          );
          return _buildTestable(
            channel: channel,
            personalChat: true,
            organization: Stream.value(const MeGroups()),
            messages: events,
            signingPubkey: 'self',
            sender: sender,
            soloSender: topLevelSender,
            relaySessionNotifier: relay,
            threadReplies: {
              threadIdRoot: [events.last],
              threadIdA: const [],
            },
            home: ThreadDetailPage(
              threadHead: messages.last,
              allMessages: messages,
              channelId: _channelId,
              currentPubkey: 'self',
              isMember: true,
              isArchived: false,
              initialMessageId: initialTarget ? threadIdA : null,
            ),
          );
        }

        await tester.pumpWidget(build());
        await tester.pumpAndSettle();
        expect(find.byType(ThreadDetailPage), findsOneWidget);
        final composer = find.byType(ComposeBar);
        expect(composer.hitTestable(), findsOneWidget);
        await tester.tap(find.text('Reply in thread…').hitTestable());
        await tester.pumpAndSettle();
        final textField = find
            .descendant(of: composer, matching: find.byType(TextField))
            .first;
        await tester.enterText(textField, 'Keep this nested draft');
        await tester.pump();
        relay.setReconnecting();
        await tester.pump();
        relay.connect();
        await tester.pumpAndSettle();
        expect(
          tester.widget<TextField>(textField).controller!.text,
          'Keep this nested draft',
        );
        // Dispose the route and provider container so restoration reads storage.
        await tester.pumpWidget(const SizedBox());
        await tester.pumpAndSettle();
        await tester.pumpWidget(build());
        await tester.pumpAndSettle();
        final restored = find.byType(ComposeBar);
        await tester.tap(
          find
              .descendant(
                of: restored,
                matching: find.text('Keep this nested draft'),
              )
              .hitTestable(),
        );
        await tester.pumpAndSettle();
        expect(
          tester
              .widget<TextField>(
                find
                    .descendant(of: restored, matching: find.byType(TextField))
                    .first,
              )
              .controller!
              .text,
          'Keep this nested draft',
        );
        final compose = tester.widget<ComposeBar>(restored);
        expect(compose.threadHeadId, threadIdA);
        expect(compose.rootId, threadIdRoot);
        await compose.onSend('Nested text', ['agent']);
        const media = <List<String>>[
          [
            'imeta',
            'url https://example.org/reply.png',
            'm image/png',
            'alt Branch attachment',
          ],
        ];
        await compose.onSend(
          '![Branch attachment](https://example.org/reply.png)',
          ['agent'],
          mediaTags: media,
        );
        expect(sender.parents, [threadIdA, threadIdA]);
        expect(sender.roots, [threadIdRoot, threadIdRoot]);
        expect(sender.channelIds, [_channelId, _channelId]);
        expect(sender.channels.every((value) => value?.isSolo == true), isTrue);
        expect(sender.mentions, [
          ['agent'],
          ['agent'],
        ]);
        expect(sender.attachments, [const <List<String>>[], media]);
        expect(topLevelSender.sent, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );
  }

  for (final inThread in [false, true]) {
    for (final kind in ['image', 'stack', 'video', 'native-stack']) {
      testWidgets(
        'Solo stream $kind media Reply opens writable branch: inThread=$inThread',
        (tester) async {
          final native = kind == 'native-stack';
          if (native) debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
          addTearDown(() => debugDefaultTargetPlatformOverride = null);
          tester.view.physicalSize = const Size(430, 900);
          tester.view.devicePixelRatio = 1;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          final channel = _soloThreadChannel();
          const first = 'https://example.org/solo-first.png';
          const second = 'https://example.org/solo-second.png';
          const video = 'https://example.org/solo-video.mp4';
          final stack = kind.endsWith('stack');
          final events = [
            if (inThread)
              _textMsg(
                id: threadIdRoot,
                pubkey: 'self',
                content: 'Outer media question',
              ),
            _textMsg(
              id: threadIdA,
              pubkey: 'agent',
              createdAt: 1001,
              content: kind == 'video'
                  ? '![Video]($video)'
                  : '![First]($first)${stack ? '\n![Second]($second)' : ''}',
              extraTags: [
                if (inThread) ...const [
                  ['e', threadIdRoot, '', 'root'],
                  ['e', threadIdRoot, '', 'reply'],
                ],
                if (kind == 'video')
                  const ['imeta', 'url $video', 'm video/mp4']
                else ...[
                  const ['imeta', 'url $first', 'm image/png'],
                  if (stack) const ['imeta', 'url $second', 'm image/png'],
                ],
              ],
            ),
          ];
          final messages = formatTimeline(events);
          final sender = _RecordingOrdinarySender();
          Map<dynamic, dynamic>? presented;
          final messenger = tester.binding.defaultBinaryMessenger;
          if (native) {
            const upload = MethodChannel('buzz/media_upload');
            final platformChannels = <MethodChannel>[];
            messenger.setMockMethodCallHandler(upload, (call) async {
              if (call.method == 'clipboardHasImage') return false;
              return null;
            });
            messenger.setMockMethodCallHandler(SystemChannels.platform_views, (
              call,
            ) async {
              if (call.method == 'create') {
                final args = call.arguments as Map;
                final channel = MethodChannel(
                  '${args['viewType']}/${args['id']}',
                );
                platformChannels.add(channel);
                messenger.setMockMethodCallHandler(channel, (_) async => null);
              }
              return null;
            });
            addTearDown(() async {
              await tester.pumpWidget(const SizedBox());
              await tester.pumpAndSettle();
              messenger.setMockMethodCallHandler(upload, null);
              messenger.setMockMethodCallHandler(
                SystemChannels.platform_views,
                null,
              );
              for (final channel in platformChannels) {
                messenger.setMockMethodCallHandler(channel, null);
              }
            });
            messenger.setMockMethodCallHandler(NativePhotoZoomPreview.channel, (
              call,
            ) async {
              if (call.method == 'isSupported') return true;
              if (call.method == 'present') {
                presented = call.arguments as Map;
                return 'reply';
              }
              return null;
            });
            addTearDown(
              () => messenger.setMockMethodCallHandler(
                NativePhotoZoomPreview.channel,
                null,
              ),
            );
          }
          await tester.pumpWidget(
            _buildTestable(
              channel: channel,
              personalChat: true,
              organization: Stream.value(const MeGroups()),
              messages: events,
              signingPubkey: 'self',
              sender: sender,
              mediaClient: http_testing.MockClient(
                (_) async => kind == 'video'
                    ? http.Response('Video unavailable', 404)
                    : http.Response.bytes(_transparentPng, 200),
              ),
              threadReplies: {
                threadIdRoot: inThread ? [events.last] : const [],
                threadIdA: const [],
              },
              home: inThread
                  ? ThreadDetailPage(
                      threadHead: messages.first,
                      allMessages: messages,
                      channelId: _channelId,
                      currentPubkey: 'self',
                      isMember: true,
                      isArchived: false,
                    )
                  : ChannelDetailPage(channel: channel),
            ),
          );
          final target = kind == 'video'
              ? find.byKey(const ValueKey('message-media-video-preview:$video'))
              : stack
              ? find.byKey(const ValueKey('message-media-stack'))
              : find.byKey(
                  const ValueKey('message-media-image-preview:$first'),
                );
          expect(target, findsOneWidget);
          if (kind != 'video') {
            // Real image codecs must finish outside the fake widget clock.
            await _waitForSoloImages(tester, target, minimum: stack ? 2 : 1);
          }
          await tester.pumpAndSettle();
          await tester.tap(target);
          if (kind == 'video') {
            // Reply remains available while video playback is loading.
            await tester.pump();
            await tester.pump(const Duration(milliseconds: 400));
          } else {
            await tester.pumpAndSettle();
          }
          if (native) {
            // Image decoding and the platform reply finish outside fake time.
            for (var i = 0; i < 500 && presented == null; i++) {
              await tester.runAsync(
                () => Future<void>.delayed(const Duration(milliseconds: 10)),
              );
              await tester.pump();
            }
            expect(presented?['canReply'], isTrue);
            await tester.pumpAndSettle();
          } else {
            final reply = find.byKey(
              ValueKey(
                'message-media-${kind == 'video' ? 'video' : 'image'}-viewer-reply-thread',
              ),
            );
            expect(reply.hitTestable(), findsOneWidget);
            await tester.tap(reply);
            await tester.pumpAndSettle();
          }
          final route = tester.widget<ThreadDetailPage>(
            find.byType(ThreadDetailPage).last,
          );
          expect(route.threadHead.id, threadIdA);
          expect(route.threadHead.rootId, inThread ? threadIdRoot : isNull);
          final compose = tester.widget<ComposeBar>(
            find.byType(ComposeBar).hitTestable(),
          );
          await compose.onSend('Media branch reply', []);
          expect(sender.parents, [threadIdA]);
          expect(sender.roots, [inThread ? threadIdRoot : threadIdA]);
          expect(tester.takeException(), isNull);
          debugDefaultTargetPlatformOverride = null;
        },
        skip: kind == 'native-stack' && !NativePhotoZoomPreview.enabled,
      );
    }
  }

  for (final system in [false, true]) {
    testWidgets(
      'Solo stream unavailable reply stays disabled: system=$system',
      (tester) async {
        final event = system
            ? _systemMsg(
                id: threadIdRoot,
                payload: {'type': 'channel_archived', 'actor': 'self'},
              )
            : _textMsg(
                id: threadIdRoot,
                pubkey: 'agent',
                content: 'Unavailable history',
              );
        final message = formatTimeline([event]).single;
        final contentKey = GlobalKey();
        await tester.pumpWidget(
          _buildTestable(
            channel: _soloThreadChannel(),
            messages: [event],
            home: Scaffold(
              body: Consumer(
                builder: (context, ref, _) => Column(
                  children: [
                    MessageSwipeReply(
                      message: message,
                      allMessages: system ? [message] : null,
                      channelId: _channelId,
                      currentPubkey: 'self',
                      isMember: true,
                      isArchived: false,
                      contentKey: contentKey,
                      child: SizedBox(
                        key: contentKey,
                        width: 200,
                        height: 80,
                        child: const Text('Reply target'),
                      ),
                    ),
                    TextButton(
                      onPressed: () => showMessageActions(
                        context: context,
                        ref: ref,
                        message: message,
                        channelId: _channelId,
                        allMessages: system ? [message] : null,
                        canManageMessage: false,
                      ),
                      child: const Text('Actions'),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        await tester.drag(find.text('Reply target'), const Offset(-220, 0));
        await tester.pumpAndSettle();
        expect(find.byType(ThreadDetailPage), findsNothing);
        expect(find.byKey(const ValueKey('swipe-reply-circle')), findsNothing);
        await tester.tap(find.text('Actions'));
        await tester.pumpAndSettle();
        expect(find.byType(BottomSheet), findsOneWidget);
        expect(find.text('Reply'), findsNothing);
      },
    );
  }
}
