/*
 * Bluetooth HID, on the boards with a radio.
 *
 * One profile for now: identity 1 advertises, and the host that pairs with it
 * gets the keyboard's reports whenever USB is not configured.  Profile n will
 * be identity n; the default identity, 0, cannot be reset, which clearing a
 * profile needs.  See jolt-mesa.md, "Bluetooth on the mesa3w".
 *
 * Pairing is Just Works: no passkey callbacks are registered, so the
 * keyboard reports no input or output capability.
 */

#include <errno.h>
#include <zephyr/bluetooth/bluetooth.h>
#include <zephyr/bluetooth/conn.h>
#include <zephyr/bluetooth/gap.h>
#include <zephyr/bluetooth/hci.h>
#include <zephyr/bluetooth/services/hids.h>
#include <zephyr/bluetooth/uuid.h>
#include <zephyr/kernel.h>
#include <zephyr/settings/settings.h>
#include <zephyr/sys/byteorder.h>
#include <zephyr/usb/class/hid.h>

#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(jolt_ble, LOG_LEVEL_INF);

extern void ble_ready_callback(bool ready);
extern void ble_report_done_callback(uintptr_t epoch);

/* The identity profile 1 advertises and bonds on. */
#define PROFILE_ID 1

/* `ble_send_report`'s answer when the stack is out of buffers, as against an
 * error: the report is fine and is to be sent again once some have gone out. */
#define BLE_SEND_BUSY 1

/* The same report as on USB: boot keyboard input, with the LED output report,
 * and no report ID. */
static const uint8_t hid_report_desc[] = HID_KEYBOARD_REPORT_DESC();

static const struct bt_data ad[] = {
	BT_DATA_BYTES(BT_DATA_FLAGS, (BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR)),
	BT_DATA_BYTES(BT_DATA_GAP_APPEARANCE, BT_BYTES_LIST_LE16(BT_APPEARANCE_HID_KEYBOARD)),
	BT_DATA_BYTES(BT_DATA_UUID16_ALL, BT_UUID_16_ENCODE(BT_UUID_HIDS_VAL)),
};

static const struct bt_data sd[] = {
	BT_DATA(BT_DATA_NAME_COMPLETE, CONFIG_BT_DEVICE_NAME, sizeof(CONFIG_BT_DEVICE_NAME) - 1),
};

static void hids_set_report(struct bt_conn *conn, enum bt_hids_report_type report_type,
			    uint8_t report_id, const uint8_t *data, uint16_t len)
{
	ARG_UNUSED(conn);
	ARG_UNUSED(report_type);
	ARG_UNUSED(report_id);
	ARG_UNUSED(data);
	ARG_UNUSED(len);

	/* The host's LED state; ignored, as on USB. */
}

static void hids_ctrl_point(struct bt_conn *conn, enum bt_hids_ctrl_point cmd)
{
	ARG_UNUSED(conn);

	LOG_INF("Host %s suspend", cmd == BT_HIDS_CTRL_SUSPEND ? "entering" : "leaving");
}

static const struct bt_hids_cb hids_cb = {
	.set_report = hids_set_report,
	.ctrl_point = hids_ctrl_point,
};

static const struct bt_hids_register_param hids_param = {
	.bcd_hid = 0x0111,
	.country_code = 0,
	.flags = BT_HIDS_INFO_FLAG_NORMALLY_CONNECTABLE,
	.report_map = hid_report_desc,
	.report_map_len = sizeof(hid_report_desc),
	.input_report_ids = {0},
	.output_report_ids = {0},
	.cb = &hids_cb,
};

static void advertise(struct k_work *work)
{
	ARG_UNUSED(work);

	struct bt_le_adv_param param = BT_LE_ADV_PARAM_INIT(
		BT_LE_ADV_OPT_CONN, BT_GAP_ADV_FAST_INT_MIN_2, BT_GAP_ADV_FAST_INT_MAX_2, NULL);
	param.id = PROFILE_ID;

	int err = bt_le_adv_start(&param, ad, ARRAY_SIZE(ad), sd, ARRAY_SIZE(sd));
	if (err == -EALREADY) {
		return;
	}
	if (err != 0) {
		LOG_ERR("Advertising failed to start: %d", err);
		return;
	}
	LOG_INF("Advertising on identity %d", PROFILE_ID);
}

static K_WORK_DEFINE(advertise_work, advertise);

static void connected(struct bt_conn *conn, uint8_t err)
{
	if (err != 0) {
		LOG_WRN("Connection failed: 0x%02x %s", err, bt_hci_err_to_str(err));
		return;
	}

	LOG_INF("Connected %s", bt_conn_dst_str(conn));

	/* The HID Service needs an encrypted link; asking for it at once gets a
	 * bonded host encrypting, and a new one pairing, without waiting for
	 * the host to trip over a characteristic it cannot read. */
	if (bt_conn_set_security(conn, BT_SECURITY_L2) != 0) {
		LOG_ERR("Failed to request security");
	}
}

static void disconnected(struct bt_conn *conn, uint8_t reason)
{
	LOG_INF("Disconnected %s: 0x%02x %s", bt_conn_dst_str(conn), reason,
		bt_hci_err_to_str(reason));
	ble_ready_callback(false);
}

/* Advertising stops when a host connects; once that connection's object is
 * free again, so that there is one to connect with, it starts again. */
static void recycled(void)
{
	k_work_submit(&advertise_work);
}

static void security_changed(struct bt_conn *conn, bt_security_t level,
			     enum bt_security_err err)
{
	if (err != 0) {
		LOG_WRN("Security failed: %s level %u: %s", bt_conn_dst_str(conn), level,
			bt_security_err_to_str(err));
		return;
	}

	LOG_INF("Security: %s level %u", bt_conn_dst_str(conn), level);
	if (level >= BT_SECURITY_L2) {
		ble_ready_callback(true);
	}
}

BT_CONN_CB_DEFINE(conn_callbacks) = {
	.connected = connected,
	.disconnected = disconnected,
	.recycled = recycled,
	.security_changed = security_changed,
};

static void pairing_complete(struct bt_conn *conn, bool bonded)
{
	LOG_INF("Paired %s, %s", bt_conn_dst_str(conn), bonded ? "bonded" : "not bonded");
}

static void pairing_failed(struct bt_conn *conn, enum bt_security_err reason)
{
	LOG_WRN("Pairing failed %s: %s", bt_conn_dst_str(conn), bt_security_err_to_str(reason));
}

static struct bt_conn_auth_info_cb auth_info_cb = {
	.pairing_complete = pairing_complete,
	.pairing_failed = pairing_failed,
};

/* Make sure the profile's identity exists.  Identities are kept in settings,
 * so this only creates one on the first boot, or after the settings were
 * erased. */
static int ensure_identity(void)
{
	bt_addr_le_t addrs[CONFIG_BT_ID_MAX];
	size_t count = ARRAY_SIZE(addrs);

	bt_id_get(addrs, &count);
	while (count <= PROFILE_ID) {
		int id = bt_id_create(NULL, NULL);

		if (id < 0) {
			return id;
		}
		count = id + 1;
	}

	char addr[BT_ADDR_LE_STR_LEN];

	bt_id_get(addrs, &count);
	bt_addr_le_to_str(&addrs[PROFILE_ID], addr, sizeof(addr));
	LOG_INF("Profile 1 is %s", addr);
	return 0;
}

int ble_setup(void)
{
	int err;

	err = bt_enable(NULL);
	if (err != 0) {
		LOG_ERR("Bluetooth init failed: %d", err);
		return err;
	}

	/* The bonds and the identities, which have to be back before the
	 * identity is looked for. */
	err = settings_load();
	if (err != 0) {
		LOG_ERR("Settings load failed: %d", err);
		return err;
	}

	err = ensure_identity();
	if (err != 0) {
		LOG_ERR("Identity create failed: %d", err);
		return err;
	}

	err = bt_conn_auth_info_cb_register(&auth_info_cb);
	if (err != 0) {
		LOG_ERR("Auth info callbacks failed: %d", err);
		return err;
	}

	err = bt_hids_register(&hids_param);
	if (err != 0) {
		LOG_ERR("HID Service register failed: %d", err);
		return err;
	}

	k_work_submit(&advertise_work);
	return 0;
}

static void report_sent(struct bt_conn *conn, void *user_data)
{
	ARG_UNUSED(conn);

	ble_report_done_callback((uintptr_t)user_data);
}

/* Send an input report to the host.  The stack copies it into the
 * notification, so the buffer is free on return.  0 means `report_sent` will
 * be called, and given back `epoch`; BLE_SEND_BUSY that the report should be
 * tried again shortly; and anything else is an error. */
int ble_send_report(const uint8_t *report, uint16_t len, uintptr_t epoch)
{
	/* With one connection, every subscribed host is the host. */
	int err = bt_hids_send_report(NULL, 0, report, len, report_sent, (void *)epoch);

	if (err == -ENOMEM) {
		return BLE_SEND_BUSY;
	}
	return err;
}
