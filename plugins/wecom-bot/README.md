# WeCom project chat gateway

This optional service connects a WeCom intelligent bot to Shepherd over the official long-connection SDK. It requires outbound HTTPS/WSS access, with no public callback URL. A legacy group webhook cannot receive mentions.

The gateway answers authorized group messages through `/rag/ask/stream`. It can attach a PNG chart for supported team statistics requests, such as “最近 7 天代码提交统计图”. Images are statistics charts; incoming images and arbitrary image generation are not supported. Relative chart windows support today or the latest 1–365 days. Conversation history is retained locally per group and sender; it is separate from browser chat history.

Charts use the [native media upload protocol](https://github.com/WecomTeam/aibot-node-sdk#uploadmedia-详细说明): upload the PNG, then send an image message with its `media_id` to the authorized conversation that requested it. The text reply contains a short caption. Stream `msg_item` attachments are not used; an accepted stream acknowledgement does not confirm image delivery. Failed uploads or image sends produce an explicit error.

## Configuration

1. Create an intelligent bot in WeCom and choose **API → Long connection**. Obtain its Bot ID and Secret.
2. Create a dedicated Shepherd user, add it to the target project, and grant only `PROJECT:READ`, `RAG:READ`, `REQUIREMENT:READ`, `BUG:READ`, and `FUNCTIONAL_CASE:READ` as needed. Create a personal API key with an explicit permission subset through `POST /system/apikey/mine`. Personal keys retain project membership and visibility-group access. Do not use an administrator key.
3. Assign the bot's role to a RAG visibility group and explicitly share the intended project documents with it. Empty document visibility groups do not mean public access. Configure ingestion to preserve the intended visibility group for future documents. Do not share private user uploads by default.
4. Create a Kubernetes Secret named `shepherd-wecom-bot`, outside Git, containing `WECOM_BOT_ID`, `WECOM_BOT_SECRET`, `SHEPHERD_URL`, `SHEPHERD_API_KEY`, and a randomly generated `WECOM_SETUP_CODE`. `SHEPHERD_URL` points to the backend service, including its port.
5. Build the image from this directory with `deploy/Dockerfile`. Replace the project and image placeholders in `deploy/kubernetes.yaml`, then apply it in the backend namespace. Configure private image pull credentials in that namespace when needed.
6. Add the bot to the target group. Mention it and send `绑定 <WECOM_SETUP_CODE>` once. The code authorizes exactly one group. Confirm the binding response, then mention the bot with a question. Optional explicit group IDs can be configured in `allowedChatIds`.

Every member of an authorized group can query the bot's shared project sources. Personal histories remain separate, but replies are visible to the group. Keep binding codes and Kubernetes credentials private. The gateway does not send welcome or scheduled messages. Keep one replica with the `Recreate` strategy: simultaneous connections using the same bot credentials can disconnect each other.

The PVC stores bindings, deduplication IDs, and conversation history. It keeps up to 100 messages per sender and removes history older than 30 days on subsequent writes. Back up this private volume; do not commit it. To revoke a group, remove it from the configuration and the local binding database while the gateway is stopped, or replace the state volume and rotate the setup code. Revoke its Shepherd key to disable all access.

`/readyz` returns 200 after WeCom authentication; `/healthz` checks the process. No public Service or Ingress is required. Logs omit credentials and message contents.

## Checks

```sh
python3 -m unittest discover -s tests -v
```

Activity summaries and business queries use the same `/rag/ask/stream` backend as web chat. The backend reads authorized project records and calculates counts, Shanghai dates and person filters. Calendar weeks run Monday through Sunday. On Sunday 2026-11-08, this week is November 2–8 and last week is October 26–November 1. Explicit ISO date ranges and the latest N days are supported within the last 365 days. Replies show collection coverage and at most 15 commit links.

Report checks locate candidate evidence in collected commits and their linked Yunxiao requirements. Similar titles are candidates, not proof of the same work, a production release, test execution, or report truthfulness. Missing candidates do not mean no work occurred. The gateway does not automatically inspect code diffs, release pipelines, or unseen group history for these checks.


## Backend authorization

Backend-managed authorization is required by default (`WECOM_REMOTE_POLICY=1`). The local setup-code flow described above is available only through explicit `WECOM_REMOTE_POLICY=0` during migration. In **System parameters → RAG → WeCom conversations**, choose the connector's dedicated account, enable the channel, and associate each group chat ID or direct-message user ID with a project. The account must already belong to that project. Its API key and role still limit available data. Each message checks the current policy; unavailable policy checks fail closed. Local bindings do not override backend policy.

Group binding is disabled by default. An administrator can enable it and generate a project-specific code valid for 1–1440 minutes. Send `绑定 <code>` while mentioning the bot in the target group. Codes are single-use and stored only as SHA-256 hashes. Turning off group binding invalidates unused codes while preserving approved conversations. Disable or remove a conversation to revoke its access.

The administration page shows connection freshness, active queries, the last receipt time, and pending conversations. Status reports contain conversation and sender identifiers, without message contents. Direct messages require explicit user authorization. Histories are separated by project, conversation, and sender.

Set `SHEPHERD_PUBLIC_URL` to the public application origin so relative source links in replies become clickable URLs. Credentials remain in private deployment Secrets. See [grounded Q&A](../../docs/grounded-qa.md) for supported data and maintained onboarding knowledge.
