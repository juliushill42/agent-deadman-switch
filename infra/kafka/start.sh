#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/../.."
  pwd
)"

cd "$ROOT"

set -a
. ./.env
set +a

RUNTIME="$ROOT/.runtime/kafka"
KAFKA_VERSION="3.7.0"
SCALA_VERSION="2.13"
DIST="kafka_${SCALA_VERSION}-${KAFKA_VERSION}"
ARCHIVE="$RUNTIME/${DIST}.tgz"
KAFKA_HOME="$RUNTIME/${DIST}"
CONFIG="$RUNTIME/kraft.properties"
LOG_DIR="$RUNTIME/data"
SERVER_LOG="$RUNTIME/server.log"

KAFKA_PORT="${KAFKA_PORT:-29112}"
KAFKA_CONTROLLER_PORT="${KAFKA_CONTROLLER_PORT:-29113}"

mkdir -p   "$RUNTIME"   "$LOG_DIR"

if [ ! -x "$KAFKA_HOME/bin/kafka-server-start.sh" ]; then
  URL="https://archive.apache.org/dist/kafka/${KAFKA_VERSION}/${DIST}.tgz"

  echo     "Downloading Kafka ${KAFKA_VERSION}"

  curl     -fL     --retry 4     --retry-delay 3     "$URL"     -o "$ARCHIVE"

  tar -tzf     "$ARCHIVE"     >/dev/null

  tar -xzf     "$ARCHIVE"     -C "$RUNTIME"
fi

cat > "$CONFIG" <<KAFKA_CONFIG
process.roles=broker,controller
node.id=1
controller.quorum.voters=1@127.0.0.1:$KAFKA_CONTROLLER_PORT
listeners=PLAINTEXT://127.0.0.1:$KAFKA_PORT,CONTROLLER://127.0.0.1:$KAFKA_CONTROLLER_PORT
advertised.listeners=PLAINTEXT://127.0.0.1:$KAFKA_PORT
controller.listener.names=CONTROLLER
listener.security.protocol.map=CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT
inter.broker.listener.name=PLAINTEXT
log.dirs=$LOG_DIR
num.partitions=3
default.replication.factor=1
offsets.topic.replication.factor=1
transaction.state.log.replication.factor=1
transaction.state.log.min.isr=1
group.initial.rebalance.delay.ms=0
auto.create.topics.enable=false
KAFKA_CONFIG

if [ ! -f "$LOG_DIR/meta.properties" ]; then
  CLUSTER_ID="$(
    "$KAFKA_HOME/bin/kafka-storage.sh"       random-uuid
  )"

  "$KAFKA_HOME/bin/kafka-storage.sh"     format     -t "$CLUSTER_ID"     -c "$CONFIG"     >/dev/null
fi

if ! "$KAFKA_HOME/bin/kafka-broker-api-versions.sh"   --bootstrap-server   "127.0.0.1:$KAFKA_PORT"   >/dev/null 2>&1
then
  nohup     "$KAFKA_HOME/bin/kafka-server-start.sh"     "$CONFIG"     >"$SERVER_LOG" 2>&1 &

  echo $!     > "$RUNTIME/kafka.pid"
fi

for _ in $(seq 1 60); do
  if "$KAFKA_HOME/bin/kafka-broker-api-versions.sh"     --bootstrap-server     "127.0.0.1:$KAFKA_PORT"     >/dev/null 2>&1
  then
    break
  fi

  sleep 1
done

"$KAFKA_HOME/bin/kafka-broker-api-versions.sh"   --bootstrap-server   "127.0.0.1:$KAFKA_PORT"   >/dev/null 2>&1 || {
    echo "Kafka failed."
    tail -n 100       "$SERVER_LOG" || true
    exit 1
  }

"$KAFKA_HOME/bin/kafka-topics.sh"   --bootstrap-server   "127.0.0.1:$KAFKA_PORT"   --create   --if-not-exists   --topic agent-deadman-events   --partitions 3   --replication-factor 1   >/dev/null

echo   "Kafka KRaft ready 127.0.0.1:$KAFKA_PORT"
