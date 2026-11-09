"""Upload chart images through the native WeCom long-connection media protocol."""
import asyncio
import base64
import hashlib
import uuid


class ImageDeliveryError(RuntimeError):
    pass


def confirm_ack(ack):
    if not isinstance(ack, dict) or ack.get("errcode") != 0:
        raise ImageDeliveryError("media_ack_not_confirmed")
    return ack.get("body") or {}


async def upload_image(client, item, index):
    try:
        raw = base64.b64decode(item["image"]["base64"], validate=True)
        digest = hashlib.md5(raw).hexdigest()
        if not raw or len(raw) > 2_000_000 or digest != item["image"]["md5"]:
            raise ImageDeliveryError("invalid_chart_image")
        async def command(name, body):
            frame = {"headers": {"req_id": name+"_"+uuid.uuid4().hex}}
            return confirm_ack(await client.reply(frame, body, name))
        chunk_size = 512 * 1024
        chunks = (len(raw)+chunk_size-1)//chunk_size
        result = await command("aibot_upload_media_init", {
            "type": "image", "filename": f"chart-{index}.png", "total_size": len(raw),
            "total_chunks": chunks, "md5": digest,
        })
        upload_id = result.get("upload_id")
        if not isinstance(upload_id, str) or not upload_id:
            raise ImageDeliveryError("upload_id_missing")
        for part in range(chunks):
            body = {"upload_id": upload_id, "chunk_index": part,
                    "base64_data": base64.b64encode(raw[part*chunk_size:(part+1)*chunk_size]).decode()}
            for attempt in range(3):
                try:
                    await command("aibot_upload_media_chunk", body)
                    break
                except Exception:
                    if attempt == 2:
                        raise
                    await asyncio.sleep(0.5*(attempt+1))
        result = await command("aibot_upload_media_finish", {"upload_id": upload_id})
        media_id = result.get("media_id")
        if not isinstance(media_id, str) or not media_id or result.get("type", "image") != "image":
            raise ImageDeliveryError("image_media_id_missing")
        print(f"image_uploaded bytes={len(raw)} chunks={chunks}", flush=True)
        return media_id
    except Exception as error:
        raise ImageDeliveryError("image_upload_failed") from error
