FROM python:3.12-slim
WORKDIR /app
COPY app.py counts.py report.py ./
RUN useradd --system --uid 10001 telemetry && mkdir /data && chown telemetry /data
USER telemetry
ENV TELEMETRY_DB=/data/telemetry.sqlite PORT=8080
EXPOSE 8080
CMD ["python", "-u", "app.py"]
