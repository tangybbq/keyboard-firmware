/*
 * Minder over Bluetooth: a GATT service carrying the minder protocol, beside
 * the HID Service on the same connection.  See jolt-mesa.md, "Minder over
 * Bluetooth".
 *
 * Two characteristics: the host writes requests to one, and the keyboard
 * notifies replies on the other.  Both need an encrypted link, so only a
 * bonded host reaches them.  Each write and each notification is one packet
 * of minder's packet framing: up to 64 bytes, the first the packet's sequence
 * number, with the top bit set on a message's last.  ATT keeps the boundaries
 * of writes and notifications, so a packet always arrives whole.
 *
 * As minder_usb.c does, this reassembles a request and hands Rust the whole
 * of it, and takes a whole reply from Rust and splits it.  A reply goes out
 * with at most MINDER_IN_FLIGHT notifications waiting, so that a key report
 * queued behind it waits for no more than that.  Sending, and every change
 * to a reply's state, happens on the system workqueue, where the completions
 * also run and where a notification never waits for a buffer.  A reply's
 * packets need an ATT MTU of 67; a host that has not offered that much gets
 * no reply, and a line in the log.
 */

#include <errno.h>
#include <string.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/gatt.h>
#include <zephyr/bluetooth/uuid.h>
#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/atomic.h>

LOG_MODULE_REGISTER(jolt_minder_ble, LOG_LEVEL_INF);

/* In minder_ble.rs. */
extern void minder_ble_read_callback(size_t len);
extern void minder_ble_write_callback(int result);

/* Bigger than the biggest message either way, as in minder_usb.c. */
#define MINDER_BUF_SIZE 4224

/* A packet: the sequence byte, then up to 63 bytes of the message. */
#define MINDER_PACKET 64
#define MINDER_PAYLOAD (MINDER_PACKET - 1)
#define MINDER_LAST 0x80
#define MINDER_SEQ 0x7f

/* A reply's notifications waiting to go out at once. */
#define MINDER_IN_FLIGHT 2

/* How long to wait before trying a notification again when the stack had no
 * buffer for it. */
#define MINDER_RETRY K_MSEC(5)

/* The service and its characteristics: 9fa708NN-5fd7-4be9-b4c7-0e6e6fcf33f3,
 * NN 00, 01 and 02.  The hosts have the same three. */
#define MINDER_UUID(n)                                                                             \
	BT_UUID_128_ENCODE(0x9fa70800 + (n), 0x5fd7, 0x4be9, 0xb4c7, 0x0e6e6fcf33f3)

static const struct bt_uuid_128 minder_service_uuid = BT_UUID_INIT_128(MINDER_UUID(0));
static const struct bt_uuid_128 minder_request_uuid = BT_UUID_INIT_128(MINDER_UUID(1));
static const struct bt_uuid_128 minder_reply_uuid = BT_UUID_INIT_128(MINDER_UUID(2));

/*
 * The request being reassembled.  Only the Bluetooth RX thread writes it, in
 * write_request() and minder_disconnected(), and only while no whole request
 * is held for Rust.
 */
static uint8_t request[MINDER_BUF_SIZE];
static size_t request_len;
/* The sequence number the next packet must have, or -1 while discarding up
 * to the start of the next message. */
static int request_seq = -1;
/* A whole request, waiting for Rust to copy it. */
static atomic_t request_held;

/*
 * The reply being sent.  Rust fills `reply` and `reply_len` while it holds
 * REPLY_BUSY, and the rest belongs to the system workqueue.
 */
enum {
	/* A reply is being sent; Rust waits for its result. */
	REPLY_BUSY,
	/* Rust has handed over a new reply, not yet begun. */
	REPLY_START,
	/* The host disconnected; end the reply under way. */
	REPLY_ABORT,
};

static atomic_t reply_flags;
static uint8_t reply[MINDER_BUF_SIZE];
static size_t reply_len;
/* How much of the reply has been queued as notifications. */
static size_t reply_sent;
static uint8_t reply_seq;
static int reply_in_flight;
/* Counts replies and disconnections, so that a completion left from an
 * earlier one is not counted against this one. */
static uintptr_t reply_epoch;

static ssize_t write_request(struct bt_conn *conn, const struct bt_gatt_attr *attr,
			     const void *buf, uint16_t len, uint16_t offset, uint8_t flags);

BT_GATT_SERVICE_DEFINE(minder_svc,
	BT_GATT_PRIMARY_SERVICE(&minder_service_uuid.uuid),
	BT_GATT_CHARACTERISTIC(&minder_request_uuid.uuid,
			       BT_GATT_CHRC_WRITE | BT_GATT_CHRC_WRITE_WITHOUT_RESP,
			       BT_GATT_PERM_WRITE_ENCRYPT, NULL, write_request, NULL),
	BT_GATT_CHARACTERISTIC(&minder_reply_uuid.uuid, BT_GATT_CHRC_NOTIFY, BT_GATT_PERM_NONE,
			       NULL, NULL, NULL),
	BT_GATT_CCC(NULL, BT_GATT_PERM_READ | BT_GATT_PERM_WRITE_ENCRYPT),
);

/* The reply characteristic's value, which notifications are sent from. */
#define REPLY_ATTR (&minder_svc.attrs[4])

static ssize_t write_request(struct bt_conn *conn, const struct bt_gatt_attr *attr,
			     const void *buf, uint16_t len, uint16_t offset, uint8_t flags)
{
	ARG_UNUSED(conn);
	ARG_UNUSED(attr);
	ARG_UNUSED(flags);

	const uint8_t *packet = buf;

	if (offset != 0 || len < 1 || len > MINDER_PACKET) {
		return BT_GATT_ERR(BT_ATT_ERR_INVALID_ATTRIBUTE_LEN);
	}
	if (atomic_get(&request_held)) {
		/* The host sent another request without waiting for the reply
		 * to the last, which the protocol never does. */
		LOG_WRN("Minder request while one is waiting; dropped");
		request_seq = -1;
		return len;
	}

	uint8_t seq = packet[0] & MINDER_SEQ;

	if (seq == 0) {
		request_len = 0;
		request_seq = 0;
	}
	if (seq != request_seq) {
		request_seq = -1;
		return len;
	}
	if (request_len + (len - 1) > sizeof(request)) {
		LOG_WRN("Minder request too big; dropped");
		request_seq = -1;
		return len;
	}

	memcpy(&request[request_len], &packet[1], len - 1);
	request_len += len - 1;

	if (packet[0] & MINDER_LAST) {
		request_seq = -1;
		atomic_set(&request_held, 1);
		minder_ble_read_callback(request_len);
	} else {
		/* The size check ends a message long before the sequence
		 * number could wrap. */
		request_seq++;
	}
	return len;
}

/* Copy out the request held, and let the next one in. */
size_t minder_ble_read_copy(uint8_t *dst, size_t cap)
{
	size_t len = MIN(request_len, cap);

	memcpy(dst, request, len);
	atomic_clear(&request_held);
	return len;
}

static void reply_work_handler(struct k_work *work);

static K_WORK_DELAYABLE_DEFINE(reply_work, reply_work_handler);

static void reply_finish(int result)
{
	atomic_clear_bit(&reply_flags, REPLY_BUSY);
	minder_ble_write_callback(result);
}

static void reply_sent_cb(struct bt_conn *conn, void *user_data)
{
	ARG_UNUSED(conn);

	if ((uintptr_t)user_data != reply_epoch || !atomic_test_bit(&reply_flags, REPLY_BUSY)) {
		return;
	}
	reply_in_flight--;
	if (reply_sent == reply_len && reply_in_flight == 0) {
		reply_finish(0);
		return;
	}
	k_work_reschedule(&reply_work, K_NO_WAIT);
}

static void find_conn(struct bt_conn *conn, void *data)
{
	struct bt_conn **found = data;

	if (*found == NULL) {
		*found = bt_conn_ref(conn);
	}
}

/* Queue as much of the reply as the in-flight limit allows. */
static void reply_send(void)
{
	struct bt_conn *conn = NULL;

	bt_conn_foreach(BT_CONN_TYPE_LE, find_conn, &conn);
	if (conn == NULL || !bt_gatt_is_subscribed(conn, REPLY_ATTR, BT_GATT_CCC_NOTIFY)) {
		reply_finish(-ENOTCONN);
		goto out;
	}
	if (bt_gatt_get_mtu(conn) < MINDER_PACKET + 3) {
		LOG_WRN("Minder needs an ATT MTU of %u, the host offered %u", MINDER_PACKET + 3,
			bt_gatt_get_mtu(conn));
		reply_finish(-EMSGSIZE);
		goto out;
	}

	/* `reply_sent == reply_len` once the last packet is queued; an empty
	 * reply, which CBOR never makes, would still send its one packet. */
	while (reply_in_flight < MINDER_IN_FLIGHT && (reply_sent < reply_len || reply_seq == 0)) {
		size_t chunk = MIN(MINDER_PAYLOAD, reply_len - reply_sent);
		bool last = reply_sent + chunk == reply_len;
		uint8_t packet[MINDER_PACKET];

		packet[0] = reply_seq | (last ? MINDER_LAST : 0);
		memcpy(&packet[1], &reply[reply_sent], chunk);

		struct bt_gatt_notify_params params = {
			.attr = REPLY_ATTR,
			.data = packet,
			.len = chunk + 1,
			.func = reply_sent_cb,
			.user_data = (void *)reply_epoch,
		};
		int err = bt_gatt_notify_cb(conn, &params);

		if (err == -ENOMEM) {
			k_work_reschedule(&reply_work, MINDER_RETRY);
			break;
		}
		if (err != 0) {
			reply_finish(err);
			break;
		}
		reply_sent += chunk;
		reply_seq = (reply_seq + 1) & MINDER_SEQ;
		reply_in_flight++;
		if (last) {
			break;
		}
	}

out:
	if (conn != NULL) {
		bt_conn_unref(conn);
	}
}

static void reply_work_handler(struct k_work *work)
{
	ARG_UNUSED(work);

	if (atomic_test_and_clear_bit(&reply_flags, REPLY_ABORT)) {
		/* Completions still to come belong to the connection that
		 * went.  A reply handed over since finds out for itself. */
		reply_epoch++;
		if (atomic_test_bit(&reply_flags, REPLY_BUSY) &&
		    !atomic_test_bit(&reply_flags, REPLY_START)) {
			reply_finish(-ENOTCONN);
		}
	}
	if (atomic_test_and_clear_bit(&reply_flags, REPLY_START)) {
		reply_epoch++;
		reply_sent = 0;
		reply_seq = 0;
		reply_in_flight = 0;
	}
	if (atomic_test_bit(&reply_flags, REPLY_BUSY)) {
		reply_send();
	}
}

/* Send a reply.  0 means minder_ble_write_callback() will be called with the
 * result; anything else is an error, and it will not be. */
int minder_ble_write(const uint8_t *src, size_t len)
{
	if (len > sizeof(reply)) {
		return -EMSGSIZE;
	}
	if (atomic_test_and_set_bit(&reply_flags, REPLY_BUSY)) {
		return -EBUSY;
	}
	memcpy(reply, src, len);
	reply_len = len;
	atomic_set_bit(&reply_flags, REPLY_START);
	k_work_reschedule(&reply_work, K_NO_WAIT);
	return 0;
}

static void minder_disconnected(struct bt_conn *conn, uint8_t reason)
{
	ARG_UNUSED(conn);
	ARG_UNUSED(reason);

	/* A request half received is gone with the host.  One held whole is
	 * left for Rust, whose reply then fails. */
	request_seq = -1;
	atomic_set_bit(&reply_flags, REPLY_ABORT);
	k_work_reschedule(&reply_work, K_NO_WAIT);
}

BT_CONN_CB_DEFINE(minder_conn_callbacks) = {
	.disconnected = minder_disconnected,
};
