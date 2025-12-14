#!/bin/sh
set -e

CONFIG=/mosquitto/config/mosquitto.conf
CERT_DIR=/mosquitto/certs

mkdir -p "$CERT_DIR" /mosquitto/data /mosquitto/log

TLS_ENABLED=false
if [ -n "$SERVER_CERT" ] && [ -n "$SERVER_KEY" ]; then
    printf '%s\n' "$SERVER_CERT" > "$CERT_DIR/server.crt"
    printf '%s\n' "$SERVER_KEY"  > "$CERT_DIR/server.key"
    chmod 600 "$CERT_DIR/server.key"

    if [ -n "$CA_CERT" ]; then
        printf '%s\n' "$CA_CERT" > "$CERT_DIR/ca.crt"
    fi

    TLS_ENABLED=true
fi

cat > "$CONFIG" <<'EOF'
allow_anonymous true

log_dest stdout
log_type error
log_type warning
log_type notice
log_type information
connection_messages true

listener 1883 0.0.0.0
protocol mqtt

listener 9001 0.0.0.0
protocol websockets
EOF

if [ "$TLS_ENABLED" = "true" ] && [ -f "$CERT_DIR/ca.crt" ]; then
    cat >> "$CONFIG" <<EOF

listener 8883 0.0.0.0
protocol mqtt
cafile $CERT_DIR/ca.crt
certfile $CERT_DIR/server.crt
keyfile $CERT_DIR/server.key
require_certificate true
EOF
else
    cat >> "$CONFIG" <<'EOF'

listener 8883 0.0.0.0
protocol mqtt
EOF
fi

health_loop() {
    while true; do
        printf 'HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK' | nc -l -p 8080 > /dev/null 2>&1
    done
}
health_loop &

exec mosquitto -c "$CONFIG"
