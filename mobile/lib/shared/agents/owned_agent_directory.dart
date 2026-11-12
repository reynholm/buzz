import 'dart:async';
import 'dart:convert';

import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../crypto/event_signing.dart';
import '../crypto/nip_oa.dart';
import '../mentions/agent_identity_provider.dart';
import '../relay/relay.dart';

/// Public model information accompanying a verified owned agent profile.
class OwnedAgentDirectoryEntry extends AgentDirectoryEntry {
  const OwnedAgentDirectoryEntry({
    required super.pubkey,
    super.displayName,
    super.avatarUrl,
    this.model,
    this.runtime,
  });
  final String? model;

  /// Runtime from the verified owner-signed registration in this community.
  final String? runtime;
}

/// The owner's named agents in the active community, including legacy fork
/// registrations without runtime metadata. Signed profiles prove ownership;
/// verified registration metadata supplies optional runtime and model details.
final ownedAgentDirectoryProvider =
    FutureProvider<List<OwnedAgentDirectoryEntry>>((ref) async {
      ref.watch(relayConfigProvider);
      final owner = ref.watch(myPubkeyProvider);
      final session = ref.watch(relaySessionProvider);
      if (owner == null) return const [];
      // Stay loading until connected: an empty list would read as "no
      // listed agents" and hide the owner's agents from mentions. The
      // provider rebuilds when the session status changes.
      if (session.status != SessionStatus.connected) {
        return Completer<List<OwnedAgentDirectoryEntry>>().future;
      }
      final relay = ref.read(relaySessionProvider.notifier);
      return loadOwnedAgentDirectory(
        owner,
        (filter) => relay.queryRelay([filter]),
      );
    });

/// Registration pages read before failing, as `loadMeAgentInventory` bounds
/// the same inventory.
const ownedAgentDirectoryPageBudget = 50;

/// Uses the active community's authenticated query boundary and paginates it.
Future<List<OwnedAgentDirectoryEntry>> loadOwnedAgentDirectory(
  String owner,
  Future<List<NostrEvent>> Function(NostrFilter) query,
) async {
  final registrations = <String, NostrEvent>{};
  final seen = <String>{};
  for (var page = 1; ; page++) {
    if (page > ownedAgentDirectoryPageBudget) {
      throw StateError('Agent inventory exceeds the read budget.');
    }
    final events = await query(
      NostrFilter(
        kinds: const [30177],
        authors: [owner],
        limit: 100,
        extensions: {'page': page},
      ),
    );
    final fresh = events.where((event) => seen.add(event.id)).toList();
    for (final event in fresh) {
      if (event.kind != 30177 || event.pubkey != owner) continue;
      final key = _tagValue(event, 'd');
      if (key == null || !RegExp(r'^[0-9a-f]{64}$').hasMatch(key)) continue;
      final previous = registrations[key];
      if (previous == null ||
          event.createdAt > previous.createdAt ||
          (event.createdAt == previous.createdAt &&
              event.id.compareTo(previous.id) < 0)) {
        registrations[key] = event;
      }
    }
    if (events.length < 100 || fresh.isEmpty) break;
  }
  final keys = {
    for (final MapEntry(:key, :value) in registrations.entries)
      if (isListedAgentRegistration(value)) key,
  };
  final results = <OwnedAgentDirectoryEntry>[];
  final ordered = keys.toList();
  for (var offset = 0; offset < ordered.length; offset += 50) {
    final batch = ordered.skip(offset).take(50).toList();
    final events = await query(NostrFilters.profilesBatch(batch));
    final latest = <String, NostrEvent>{};
    for (final event in events) {
      if (event.kind != 0 || !batch.contains(event.pubkey)) continue;
      final previous = latest[event.pubkey];
      if (previous == null ||
          event.createdAt > previous.createdAt ||
          (event.createdAt == previous.createdAt &&
              event.id.compareTo(previous.id) < 0)) {
        latest[event.pubkey] = event;
      }
    }
    for (final event in latest.values) {
      if (verifiedProfileOaOwnerPubkey(event) == owner) {
        final profile = AgentDirectoryEntry.fromEvent(event);
        String? model;
        String? runtime;
        try {
          final registration = registrations[event.pubkey]!;
          // Native libsecp256k1 verification; the pure-Dart codec took
          // several milliseconds per registration on the UI isolate.
          verifiedNostrEventFromJson(jsonEncode(registration.toJson()));
          runtime = _tagValue(registration, 'runtime');
          final data = jsonDecode(registration.content);
          if (data is Map && data['model'] is String) {
            final value = (data['model'] as String).trim();
            if (value.isNotEmpty) model = value;
          }
        } catch (_) {
          /* Older registrations may have no metadata. */
        }
        results.add(
          OwnedAgentDirectoryEntry(
            pubkey: profile.pubkey,
            displayName: profile.displayName,
            avatarUrl: profile.avatarUrl,
            model: model,
            runtime: runtime,
          ),
        );
      }
    }
  }
  return results;
}

/// An agent type key, `pluginId/typeId`, as desktop's `RUNTIME` pattern
/// (buzz-app `src/features/agents/service.ts`) accepts it.
final _runtimePattern = RegExp(r'^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*$');

/// Whether desktop lists this kind:30177 registration as one of the owner's
/// agents: it names the agent, and any runtime metadata has a valid type key.
/// Legacy fork registrations may omit runtime without losing their visibility.
bool isListedAgentRegistration(NostrEvent event) {
  final runtime = _tagValue(event, 'runtime');
  if (runtime != null && !_runtimePattern.hasMatch(runtime)) {
    return false;
  }
  try {
    final content = jsonDecode(event.content);
    return content is Map &&
        content['name'] is String &&
        (content['name'] as String).trim().isNotEmpty;
  } on FormatException {
    return false;
  }
}

String? _tagValue(NostrEvent event, String name) {
  for (final tag in event.tags) {
    if (tag.length >= 2 && tag.first == name) return tag[1];
  }
  return null;
}
