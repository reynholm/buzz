import 'dart:async';
import 'dart:convert';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:buzz/shared/community/community_provider.dart';
import 'package:buzz/shared/agents/remote_agents.dart';
import 'package:buzz/shared/hosted/builderlab_account.dart';
import 'package:buzz/shared/hosted/hosted_relay_auth.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:buzz/shared/community/community.dart';
import 'package:buzz/shared/hosted/account_identity_store.dart';
import 'package:buzz/shared/hosted/hosted_environment.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:shared_preferences/shared_preferences.dart';
import 'package:shared_preferences_platform_interface/shared_preferences_platform_interface.dart';
import '../../helpers/remote_agents_fixture.dart';

class JournalGate extends InMemorySharedPreferencesStore {
  JournalGate() : super.empty();
  final entered = Completer<void>();
  final release = Completer<void>();
  bool gated = false;
  @override
  Future<bool> setValue(String type, String key, Object value) async {
    if (!gated &&
        key.endsWith(':pending') &&
        value is String &&
        jsonDecode(value)['registering'] == true) {
      gated = true;
      entered.complete();
      await release.future;
    }
    return super.setValue(type, key, value);
  }
}

class _IdentityReadGate extends FlutterSecureStorage {
  bool armed = false;
  final entered = Completer<void>();
  final release = Completer<void>();

  @override
  Future<String?> read({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    final value = await super.read(
      key: key,
      iOptions: iOptions,
      aOptions: aOptions,
      lOptions: lOptions,
      webOptions: webOptions,
      mOptions: mOptions,
      wOptions: wOptions,
    );
    if (armed && key.startsWith(AccountIdentityStore.storageKey)) {
      armed = false;
      entered.complete();
      await release.future;
    }
    return value;
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  for (final transition in [
    'different account',
    'signed out',
    'new session',
    'community away and back',
  ]) {
    test('creation journal retires on $transition and remains resumable', () async {
      FlutterSecureStorage.setMockInitialValues({});
      SharedPreferences.setMockInitialValues({});
      final store = JournalGate();
      final identityStorage = _IdentityReadGate();
      SharedPreferencesStorePlatform.instance = store;
      final second = nostr.Keys('0' * 63 + '3');
      await AccountIdentityStore()
          .forSubject(fakeSubject)
          .create(fakeOwner.nsec);
      await AccountIdentityStore()
          .forSubject('second-owner')
          .create(second.nsec);
      final mutations = <String>[];
      final expiry = DateTime.now()
          .toUtc()
          .add(const Duration(days: 1))
          .toIso8601String();
      final transport = MockClient((request) async {
        final path = request.url.path;
        final body = request.body.isEmpty
            ? <String, dynamic>{}
            : jsonDecode(request.body) as Map<String, dynamic>;
        final credential = request.headers['X-BB-Session-Credential'];
        final subject = credential == 'session-B'
            ? 'second-owner'
            : fakeSubject;
        Object result;
        if (path.endsWith('/exchange')) {
          result = {
            'session_credential': body['code'] == 'B'
                ? 'session-B'
                : 'session-A',
            'expires_at': expiry,
          };
        } else if (path.endsWith('/auth/me')) {
          result = {'subject': subject, 'expires_at': expiry};
        } else if (path.endsWith('/nostr-identities/current')) {
          result = {
            'identity': {
              'pubkey_hex': subject == fakeSubject
                  ? fakeOwner.public
                  : second.public,
            },
          };
        } else if (path.endsWith('/get-model-catalog')) {
          result = {
            'models': [
              {
                'id': 'test-model',
                'display_name': 'Test',
                'supported_reasoning_efforts': ['high'],
              },
            ],
          };
        } else if (path.endsWith('/list-agents')) {
          result = {'status': 1, 'agents': []};
        } else {
          mutations.add(
            '$subject:${path.split('/').last}:${body['agent_instructions'] ?? ''}',
          );
          result = path.endsWith('/register-agent')
              ? {'status': 1, 'agent_pubkey': fakeAgent, 'agent_id': 'agent'}
              : path.endsWith('/attest-agent')
              ? {'status': 1}
              : {};
        }
        return http.Response(jsonEncode(result), 200);
      });
      final registry = BuilderlabAccountClient(httpClient: transport);
      await registry.exchange(code: 'A', verifier: 'a' * 43);
      final originalCommunity = Community.create(
        name: 'A',
        relayUrl: 'wss://test.${HostedEnvironment.communitySuffix}',
        pubkey: fakeOwner.public,
        nsec: fakeOwner.nsec,
      );
      var currentCommunity = originalCommunity;
      final relayAuth = HostedRelayAuth(registry);
      final container = ProviderContainer(
        overrides: [
          builderlabAccountProvider.overrideWithValue(registry),
          hostedRelayAuthProvider.overrideWithValue(relayAuth),
          activeCommunityProvider.overrideWith((ref) async => currentCommunity),
        ],
      );
      await container.read(activeCommunityProvider.future);
      final communityTransition = transition == 'community away and back';
      final controller = communityTransition
          ? container.read(remoteAgentControllerProvider)
          : RemoteAgentController(
              account: registry,
              keys: AccountIdentityStore(storage: identityStorage),
              relayAuth: relayAuth,
            );
      addTearDown(() {
        if (!communityTransition) controller.dispose();
        container.dispose();
        relayAuth.dispose();
        registry.dispose();
      });
      Future<void> changeScope() async {
        if (communityTransition) {
          for (final destination in [
            Community.create(
              name: 'Other',
              relayUrl: 'wss://other.${HostedEnvironment.communitySuffix}',
              pubkey: fakeOwner.public,
              nsec: fakeOwner.nsec,
            ),
            originalCommunity,
          ]) {
            currentCommunity = destination;
            container.invalidate(activeCommunityProvider);
            await container.read(activeCommunityProvider.future);
          }
        } else {
          if (transition != 'new session') await registry.clearLocalSession();
          if (transition != 'signed out') {
            await registry.exchange(
              code: transition == 'new session' ? 'A' : 'B',
              verifier: 'b' * 43,
            );
          }
        }
      }

      final operation = controller.create(
        originalCommunity,
        'Private Scout',
        instructions: 'FIRST ACCOUNT PRIVATE INSTRUCTIONS',
        modelId: 'test-model',
        reasoningEffort: 'high',
      );
      await store.entered.future;
      await changeScope();
      store.release.complete();
      final result = await operation;
      expect(result, isFalse);
      expect(controller.pending, isNull);
      expect(controller.created, isNull);
      final prefs = await SharedPreferences.getInstance();
      await prefs.reload();
      final originalScope =
          'remote-agents:${HostedEnvironment.origin.host}:$fakeSubject';
      final saved =
          jsonDecode(prefs.getString('$originalScope:pending')!) as Map;
      expect(saved['registering'], isTrue);
      expect(saved['owner'], fakeOwner.public);
      expect(
        prefs.getString(
          'remote-agents:${HostedEnvironment.origin.host}:second-owner:pending',
        ),
        isNull,
      );
      expect(
        mutations,
        isEmpty,
        reason: 'A retired creation must not mutate another session',
      );
      if (registry.account != null) await registry.clearLocalSession();
      await registry.exchange(code: 'A', verifier: 'a' * 43);
      if (!communityTransition) {
        identityStorage.armed = true;
        final restore = controller.run(controller.ensureBinding);
        await identityStorage.entered.future;
        await changeScope();
        identityStorage.release.complete();
        expect(await restore, isFalse);
        expect(controller.owner, isNull);
        expect(controller.pending, isNull);
        expect(controller.failure, isEmpty);
        if (registry.account != null) await registry.clearLocalSession();
        await registry.exchange(code: 'A', verifier: 'a' * 43);
      }
      await controller.ensureBinding();
      expect(controller.pending!['request'], saved['request']);
      expect(
        controller.pending!['instructions'],
        'FIRST ACCOUNT PRIVATE INSTRUCTIONS',
      );
    });
  }
}
