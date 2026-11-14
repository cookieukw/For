# ZapFast patch stack

This directory contains the local feature patches carried on top of upstream
ZapFast. Keep each patch focused, named in English, and applicable in
lexicographic order.

The patches add three account-level controls: channel notifications, delayed
read receipts, and retaining incoming messages after a revoke. They also add a
per-group sender ignore action. Ignored senders' message rows and search hits
are hidden in that group only; they are not blocked on WhatsApp.

The group information panel keeps the description collapsed until requested,
puts members before the media shelf, and provides a searchable, taller member
list. Each member's options menu can ignore or unignore them in that group.

Valid `wa.me` and `api.whatsapp.com/send` links clicked in a message open the
direct chat inside ZapFast. A shared `text=` template is added to the composer
for review and is never sent automatically.

AI reply drafting supports OpenAI, DeepSeek, Anthropic, Gemini, OpenRouter, and an
OpenAI-compatible loopback endpoint (Ollama or LM Studio). It is enabled per
direct chat, sends the configured number of recent messages (or all locally
archived messages) ending at the latest incoming message, together with that
chat's style instruction to the selected provider. The result is placed in the
composer for manual review. Provider keys are stored in the OS keyring and model
lists are fetched dynamically. Hosted providers receive this context only when
a draft is requested; users should review the provider's privacy policy. Groups
and automatic sending are excluded; the simulator uses synthetic text only.

Read receipts and linked-device read-state updates are queued by chat and
released after a successful outgoing message in that chat. The queue is
currently in memory and is cleared when an account is unlinked or the app exits
before a reply is sent.

## Apply

From the repository root, run:

```sh
./patches/apply.sh
```

The script applies patches to the current checkout and stops on the first
conflict. Start from a clean tree at the base revision recorded in
`patches/base-revision`.

## Refreshing patches

After changing the working tree, export each focused change with
`git diff --binary` into its numbered patch file. Do not include unrelated
changes in a patch. Update `patches/base-revision` when rebasing the stack onto
a newer upstream commit, then verify every patch applies from a clean checkout.

Patch files and comments are written in English to make the stack easier to
share and review.
