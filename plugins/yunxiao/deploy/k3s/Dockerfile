ARG BASE_IMAGE=debian:bookworm-slim
FROM ${BASE_IMAGE}
ARG APT_MIRROR=http://deb.debian.org
RUN sed -i "s|http://deb.debian.org|${APT_MIRROR}|g" /etc/apt/sources.list.d/debian.sources \
    && apt-get update && apt-get install -y --no-install-recommends python3 python3-pil git ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
COPY --chown=1000:1000 collector/ /opt/collector/
COPY --chown=1000:1000 scripts/ /opt/sync/
ENV HOME=/data PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 \
    SHEPHERD_COLLECTOR_PATH=/opt/collector
USER 1000:1000
WORKDIR /opt/sync
ENTRYPOINT ["python3", "/opt/sync/yunxiao_sync.py"]
