ARG BASE_IMAGE=python:3.12-slim
FROM ${BASE_IMAGE}
USER root
ARG APT_MIRROR=http://deb.debian.org
RUN if [ -f /etc/apt/sources.list.d/debian.sources ]; then sed -i "s|http://deb.debian.org|${APT_MIRROR}|g" /etc/apt/sources.list.d/debian.sources; fi \
    && apt-get update && apt-get install -y --no-install-recommends python3-venv ca-certificates fonts-noto-cjk \
    && rm -rf /var/lib/apt/lists/*
COPY requirements.txt /app/requirements.txt
ARG PIP_INDEX_URL=https://pypi.org/simple
RUN python3 -m venv /opt/venv && /opt/venv/bin/pip install --no-cache-dir -r /app/requirements.txt
COPY --chown=1000:1000 scripts/ /app/
ENV PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 WECOM_CHART_FONT=/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc
USER 1000:1000
WORKDIR /app
EXPOSE 8080
ENTRYPOINT ["/opt/venv/bin/python", "/app/gateway.py"]
