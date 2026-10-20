# Restart status turns

An authorized Restart first durably resets the selected scope and waits for
checked termination of its old workers. Only successful retirement schedules a
short task-status turn. The Restart acknowledgment confirms that reset; it does
not claim that a provider started or that a status reply reached the relay.

The status is the replacement session's first foreground turn. It uses the
selected None, Recent or All history policy, the normal standing instructions
and the original assignment where available. None does not reconstruct omitted
progress from files, logs or another provider session. The automatic bootstrap
initial message is suppressed. Later human messages keep their original queue
entries and instructions; they cannot steer or merge into this status turn.
Other scopes remain available. Status acquisition and execution have at most
120 seconds, subject to shorter configured task limits. Failure feedback has a
separate five-second grace and does not extend queue priority.

The provider returns one short text answer, without tools, subagents or task
execution. The controller captures answer chunks for that exact session, signs
one ordinary kind-9 reply as the selected agent, and posts it to the original
canonical thread or DM. It adds no mentions or broadcast tags. The controller
requires an exact relay acknowledgment before consuming the captured restart
intent. Refusal, limits, empty output, partial failed output, observed tools,
startup errors and unconfirmed delivery cannot become successful task status.
They leave an independent scoped failure notice when it can be delivered.
Restart success remains unchanged.

The turn has a durable Started marker before provider acquisition or model
execution. Captured text is limited to 12 KiB, and each saved signed event to
20 KiB, within the existing 256 KiB record cap. Signed status bytes are saved
before sending; at most three same-owner submissions reuse those exact bytes.
Failure-notice bytes are saved separately and never replace an uncertain actual
reply. New Restart, cancellation, shutdown and channel removal fence late
results. Publication authorization is the last controller check before HTTP
submission: a request already sent to the relay cannot be retracted.

After the owning controller dies, recovery never starts another model attempt
or automatically resubmits prepared bytes. It preserves those bytes and marks
the follow-up unconfirmed. This also prevents a failed cancellation write plus
a crash from authorizing stale output. A crash before dispatch or acknowledgment
can therefore lose the automatic status; a fresh Restart explicitly requests a
new attempt, and ordinary human input can still continue using the retained
history policy. Cancellation persistence failure is reported as unconfirmed,
while the live controller immediately fences further effects. If cancellation
cannot be saved, the owner/observer control result is unconfirmed and no chat
notice is sent: a failed write cannot establish publication ownership. Status
and failure-feedback jobs retain the exact scope fence captured before
scheduling, including expiry and known-local-panic feedback. A temporarily
missing or unreadable record cannot retain local status authority: cancellation
fences the accepted local token before reading the store. This also covers
cancellation
while retirement is still pending; later checked retirement can confirm the
reset and retain its history policy without recreating the cancelled follow-up.
A previously confirmed terminal status is preserved.

Language inference deliberately supports Russian, English and unknown. The
bounded selected-history reader keeps only a language code from evidence that
None otherwise omits. It checks signed channel/thread routing and ignores
controls, code, quotes and timestamp/severity log prefixes. Numbered progress
prose remains evidence. Newer substantive unsupported prose replaces an older
recognized hint with unknown; excluded non-prose does not replace it. The
configured owner and the exact authenticated requester are trusted conversation
authors by policy; this does
not prove human identity. Verified signed agent attestations exclude an author.
Absent profiles do not establish human identity. Unsupported or ambiguous
conversation evidence keeps the generic same-conversation-language instruction,
without inventing an English hint. No additional model call or unbounded
history lookup is used.

ACP permission requests for the status session are denied, and observed tool
calls fail the turn. Some providers can execute native tools before reporting
them, so this is not a universal tool sandbox. Real provider validation must
confirm the adapter's actual behavior.
