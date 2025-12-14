FROM eclipse-mosquitto:2

USER root
RUN adduser -D -u 10001 app && \
    mkdir -p /mosquitto/certs /mosquitto/config /mosquitto/data /mosquitto/log && \
    chown -R app:app /mosquitto

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

USER 10001

EXPOSE 1883 8883 9001

ENTRYPOINT ["/entrypoint.sh"]
