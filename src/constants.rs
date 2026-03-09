//! # constants
//!
//! Shared protocol constants — mirrors `src/constants.js` from the original Moleculer.js.
//! These values are used across the broker, transit, transporters, and middleware.

// ─── Circuit-breaker states ───────────────────────────────────────────────────

pub const CIRCUIT_CLOSE: &str = "close";
pub const CIRCUIT_HALF_OPEN: &str = "half_open";
pub const CIRCUIT_HALF_OPEN_WAIT: &str = "half_open_wait";
pub const CIRCUIT_OPEN: &str = "open";

// ─── Packet type identifiers (used in transit wire protocol) ─────────────────

pub const PACKET_UNKNOWN: &str = "???";
pub const PACKET_EVENT: &str = "EVENT";
pub const PACKET_REQUEST: &str = "REQ";
pub const PACKET_RESPONSE: &str = "RES";
pub const PACKET_DISCOVER: &str = "DISCOVER";
pub const PACKET_INFO: &str = "INFO";
pub const PACKET_DISCONNECT: &str = "DISCONNECT";
pub const PACKET_HEARTBEAT: &str = "HEARTBEAT";
pub const PACKET_PING: &str = "PING";
pub const PACKET_PONG: &str = "PONG";

/// Gossip protocol packets (used by Redis/etcd3 discoverers)
pub const PACKET_GOSSIP_REQ: &str = "GOSSIP_REQ";
pub const PACKET_GOSSIP_RES: &str = "GOSSIP_RES";
pub const PACKET_GOSSIP_HELLO: &str = "GOSSIP_HELLO";

// ─── Serialisation data types ─────────────────────────────────────────────────

pub const DATATYPE_UNDEFINED: u8 = 0;
pub const DATATYPE_NULL: u8 = 1;
pub const DATATYPE_JSON: u8 = 2;
pub const DATATYPE_BUFFER: u8 = 3;

// ─── Internal error / event names ────────────────────────────────────────────

/// Emitted when transit fails to process the packet.
pub const FAILED_PROCESSING_PACKET: &str = "failedProcessingPacket";

/// Emitted when transit fails to send a request packet.
pub const FAILED_SEND_REQUEST_PACKET: &str = "failedSendRequestPacket";

/// Emitted when transit fails to send an event packet.
pub const FAILED_SEND_EVENT_PACKET: &str = "failedSendEventPacket";

/// Emitted when transit fails to send a response packet.
pub const FAILED_SEND_RESPONSE_PACKET: &str = "failedSendResponsePacket";

/// Emitted when transit fails to discover multiple nodes.
pub const FAILED_NODES_DISCOVERY: &str = "failedNodesDiscovery";

/// Emitted when transit fails to discover a single node.
pub const FAILED_NODE_DISCOVERY: &str = "failedNodeDiscovery";

/// Emitted when transit fails to send an INFO packet.
pub const FAILED_SEND_INFO_PACKET: &str = "failedSendInfoPacket";

/// Emitted when transit fails to send a PING packet.
pub const FAILED_SEND_PING_PACKET: &str = "failedSendPingPacket";

/// Emitted when transit fails to send a PONG packet.
pub const FAILED_SEND_PONG_PACKET: &str = "failedSendPongPacket";

/// Emitted when transit fails to send a HEARTBEAT packet.
pub const FAILED_SEND_HEARTBEAT_PACKET: &str = "failedSendHeartbeatPacket";

/// Emitted when the broker fails to stop all services.
pub const FAILED_STOPPING_SERVICES: &str = "failedServicesStop";

/// Emitted when the broker fails to load a service.
pub const FAILED_LOAD_SERVICE: &str = "failedServiceLoad";

/// Emitted when the broker fails to restart a service.
pub const FAILED_RESTART_SERVICE: &str = "failedServiceRestart";

/// Emitted when the broker fails to destroy a service.
pub const FAILED_DESTRUCTION_SERVICE: &str = "failedServiceDestruction";

/// Emitted when a CACHER / DISCOVERER / TRANSPORTER client receives an error.
pub const CLIENT_ERROR: &str = "clientError";

/// Emitted when a Redis client fails while pinging the server.
pub const FAILED_SEND_PING: &str = "failedSendPing";

/// Emitted when the etcd3 discoverer fails to collect keys.
pub const FAILED_COLLECT_KEYS: &str = "failedCollectKeys";

/// Emitted when the etcd3 discoverer fails to send an INFO packet.
pub const FAILED_SEND_INFO: &str = "failedSendInfo";

/// Emitted when the Redis discoverer fails to scan keys.
pub const FAILED_KEY_SCAN: &str = "failedKeyScan";

/// Emitted when the Redis publisher fails.
pub const FAILED_PUBLISHER_ERROR: &str = "publisherError";

/// Emitted when the Redis consumer fails.
pub const FAILED_CONSUMER_ERROR: &str = "consumerError";

/// Emitted when Kafka fails to create topics.
pub const FAILED_TOPIC_CREATION: &str = "failedTopicCreation";

/// Emitted when AMQP fails to connect.
pub const FAILED_CONNECTION_ERROR: &str = "failedConnection";

/// Emitted when an AMQP channel error occurs.
pub const FAILED_CHANNEL_ERROR: &str = "failedChannel";

/// Emitted when AMQP fails to ACK a packet.
pub const FAILED_REQUEST_ACK: &str = "requestAck";

/// Emitted when AMQP disconnects unexpectedly.
pub const FAILED_DISCONNECTION: &str = "failedDisconnection";

/// Emitted when AMQP fails to publish a balanced event.
pub const FAILED_PUBLISH_BALANCED_EVENT: &str = "failedPublishBalancedEvent";

/// Emitted when AMQP fails to publish a balanced request.
pub const FAILED_PUBLISH_BALANCED_REQUEST: &str = "publishBalancedRequest";

// ─── Default limits / timeouts ────────────────────────────────────────────────

/// Default request timeout in milliseconds (0 = no timeout).
pub const DEFAULT_REQUEST_TIMEOUT: u64 = 0;

/// Default heartbeat interval in seconds.
pub const DEFAULT_HEARTBEAT_INTERVAL: u64 = 5;

/// Default heartbeat timeout in seconds (node is considered dead after this).
pub const DEFAULT_HEARTBEAT_TIMEOUT: u64 = 15;

/// Maximum call stack depth to prevent infinite loops.
pub const DEFAULT_MAX_CALL_LEVEL: u32 = 100;

/// Default log level.
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// Moleculer protocol version used for compatibility checks.
pub const PROTOCOL_VERSION: &str = "4";

/// Default namespace (empty means no namespace isolation).
pub const DEFAULT_NAMESPACE: &str = "";
