/*
 * Bluetooth HID, on the boards with a radio.
 *
 * A profile is one host.  Profile n is Bluetooth identity n, each with an
 * address of its own; the default identity, 0, cannot be reset, which
 * clearing a profile needs, and is unused.  Only the active profile
 * advertises, and its host gets the keyboard's reports whenever USB is not
 * configured (hid.rs decides).  The active profile is kept in settings, so it
 * survives a reset.  See jolt-mesa.md, "Bluetooth on the mesa3w".
 *
 * A profile with a bond belongs to its host: another host may connect, but
 * its pairing is refused and the link dropped, so that it can neither replace
 * the bond nor hold the one connection.  The host itself may pair again over
 * its old bond.  Clearing the profile is how it is given to a new host.
 * A host pairing on a second profile loses its bond on the first
 * (CONFIG_BT_ID_UNPAIR_MATCHING_BONDS): one host, one profile.
 *
 * Advertising and the profile commands all run on `ble_workq`, one at a time,
 * and off the system workqueue, as clearing a profile writes the flash.
 *
 * Pairing is Just Works: no passkey callbacks are registered, so the
 * keyboard reports no input or output capability.
 */

#include <errno.h>
#include <stdio.h>
#include <string.h>
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
extern void ble_connected_callback(uint8_t profile);
extern void ble_report_done_callback(uintptr_t epoch);

/* How many profiles: identities 1 to PROFILE_COUNT.  CONFIG_BT_ID_MAX is one
 * more, and CONFIG_BT_MAX_PAIRED the same, a bond each. */
#define PROFILE_COUNT 4

BUILD_ASSERT(CONFIG_BT_ID_MAX == PROFILE_COUNT + 1, "an identity per profile, and identity 0");
BUILD_ASSERT(CONFIG_BT_MAX_PAIRED >= PROFILE_COUNT, "a bond per profile");

/* Where the active profile is kept. */
#define SETTINGS_PROFILE "jolt/ble/profile"

#define BLE_WORKQ_STACK_SIZE 2048
#define BLE_WORKQ_PRIO 10

/* `ble_send_report`'s answer when the stack is out of buffers, as against an
 * error: the report is fine and is to be sent again once some have gone out. */
#define BLE_SEND_BUSY 1

/* The same report as on USB: boot keyboard input, with the LED output report,
 * and no report ID. */
static const uint8_t hid_report_desc[] = HID_KEYBOARD_REPORT_DESC();

static K_THREAD_STACK_DEFINE(ble_workq_stack, BLE_WORKQ_STACK_SIZE);
static struct k_work_q ble_workq;

/* Set once `ble_setup` has finished, so that the profile commands know there
 * is a stack to give them to. */
static bool ble_up;

/* The profile that advertises and connects.  Only changed on `ble_workq`, and
 * by settings_load() before that starts. */
static uint8_t active_profile = 1;

static int settings_set(const char *name, size_t len, settings_read_cb read_cb, void *cb_arg)
{
	const char *next;

	if (!settings_name_steq(name, "profile", &next) || next != NULL) {
		return -ENOENT;
	}

	uint8_t profile;

	if (len != sizeof(profile)) {
		return -EINVAL;
	}

	int rc = read_cb(cb_arg, &profile, sizeof(profile));

	if (rc < 0) {
		return rc;
	}
	/* A profile from a build with more of them falls back to the first. */
	if (profile >= 1 && profile <= PROFILE_COUNT) {
		active_profile = profile;
	}
	return 0;
}

SETTINGS_STATIC_HANDLER_DEFINE(jolt_ble, "jolt/ble", NULL, settings_set, NULL, NULL);

static const struct bt_data ad[] = {
	BT_DATA_BYTES(BT_DATA_FLAGS, (BT_LE_AD_GENERAL | BT_LE_AD_NO_BREDR)),
	BT_DATA_BYTES(BT_DATA_GAP_APPEARANCE, BT_BYTES_LIST_LE16(BT_APPEARANCE_HID_KEYBOARD)),
	BT_DATA_BYTES(BT_DATA_UUID16_ALL, BT_UUID_16_ENCODE(BT_UUID_HIDS_VAL)),
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

static void count_conn(struct bt_conn *conn, void *data)
{
	ARG_UNUSED(conn);

	(*(int *)data)++;
}

static void disconnect_conn(struct bt_conn *conn, void *data)
{
	ARG_UNUSED(data);

	(void)bt_conn_disconnect(conn, BT_HCI_ERR_REMOTE_USER_TERM_CONN);
}

/* Advertise the active profile, unless there is a connection, or one still
 * being torn down: `recycled` comes back here once it is gone. */
static void advertise(void)
{
	int conns = 0;

	bt_conn_foreach(BT_CONN_TYPE_LE, count_conn, &conns);
	if (conns != 0) {
		return;
	}

	/* The name carries the profile, "Jolt Keyboard 2", so that a host's list
	 * says which of the keyboard's entries is which.  A host reads the name
	 * from the advertisement, and once connected from the GAP Device Name,
	 * which is bt_set_name()'s. */
	static char name[CONFIG_BT_DEVICE_NAME_MAX + 1];

	snprintf(name, sizeof(name), "%s %u", CONFIG_BT_DEVICE_NAME, active_profile);
	if (bt_set_name(name) != 0) {
		LOG_WRN("Setting the name failed");
	}

	const struct bt_data sd[] = {
		BT_DATA(BT_DATA_NAME_COMPLETE, name, strlen(name)),
	};
	struct bt_le_adv_param param = BT_LE_ADV_PARAM_INIT(
		BT_LE_ADV_OPT_CONN, BT_GAP_ADV_FAST_INT_MIN_2, BT_GAP_ADV_FAST_INT_MAX_2, NULL);
	param.id = active_profile;

	int err = bt_le_adv_start(&param, ad, ARRAY_SIZE(ad), sd, ARRAY_SIZE(sd));
	if (err == -EALREADY) {
		return;
	}
	if (err != 0) {
		LOG_ERR("Advertising failed to start: %d", err);
		return;
	}
	LOG_INF("Advertising profile %u as \"%s\"", active_profile, name);
}

static void advertise_handler(struct k_work *work)
{
	ARG_UNUSED(work);

	advertise();
}

static K_WORK_DEFINE(advertise_work, advertise_handler);

/* The profile last asked for by `ble_select_profile`. */
static atomic_t select_target;

/* Make `select_target` the active profile.  Selecting the active one again
 * only makes sure it is advertising, if it is not connected.  Another one
 * drops the connection, and `recycled` advertises the new one once it is
 * gone. */
static void select_handler(struct k_work *work)
{
	ARG_UNUSED(work);

	uint8_t profile = atomic_get(&select_target);

	if (profile != active_profile) {
		(void)bt_le_adv_stop();
		active_profile = profile;
		LOG_INF("Profile %u selected", profile);
		bt_conn_foreach(BT_CONN_TYPE_LE, disconnect_conn, NULL);

		int err = settings_save_one(SETTINGS_PROFILE, &profile, sizeof(profile));

		if (err != 0) {
			LOG_ERR("Saving the profile failed: %d", err);
		}
	}
	advertise();
}

static K_WORK_DEFINE(select_work, select_handler);

/* The profiles `ble_clear_profile` has been asked to clear, a bit each. */
static atomic_t clear_mask;

static void log_bond(const struct bt_bond_info *info, void *data)
{
	char addr[BT_ADDR_LE_STR_LEN];

	bt_addr_le_to_str(&info->addr, addr, sizeof(addr));
	LOG_INF("Profile %u bonded to %s", *(uint8_t *)data, addr);
}

/* Log a profile's address, and the hosts it is bonded to. */
static void log_profile_addr(const char *what, uint8_t profile)
{
	bt_addr_le_t addrs[CONFIG_BT_ID_MAX];
	size_t count = ARRAY_SIZE(addrs);
	char addr[BT_ADDR_LE_STR_LEN];

	bt_id_get(addrs, &count);
	if (profile >= count) {
		return;
	}
	bt_addr_le_to_str(&addrs[profile], addr, sizeof(addr));
	LOG_INF("Profile %u %s %s", profile, what, addr);
	bt_foreach_bond(profile, log_bond, &profile);
}

/* Forget each profile in `clear_mask`: resetting its identity unpairs it,
 * drops its connection, and gives it a new address, so that the host's old
 * bond is simply dead rather than in the way of a new pairing.  A profile
 * cannot be reset while it advertises. */
static void clear_handler(struct k_work *work)
{
	ARG_UNUSED(work);

	atomic_val_t mask = atomic_clear(&clear_mask);

	for (uint8_t profile = 1; profile <= PROFILE_COUNT; profile++) {
		if ((mask & BIT(profile)) == 0) {
			continue;
		}
		if (profile == active_profile) {
			(void)bt_le_adv_stop();
		}

		int err = bt_id_reset(profile, NULL, NULL);

		if (err < 0) {
			LOG_ERR("Profile %u clear failed: %d", profile, err);
			continue;
		}
		log_profile_addr("cleared, now", profile);
	}
	advertise();
}

static K_WORK_DEFINE(clear_work, clear_handler);

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
	k_work_submit_to_queue(&ble_workq, &advertise_work);
}

static void security_changed(struct bt_conn *conn, bt_security_t level,
			     enum bt_security_err err)
{
	if (err != 0) {
		LOG_WRN("Security failed: %s level %u: %d %s", bt_conn_dst_str(conn), level, err,
			bt_security_err_to_str(err));
		/* A link that cannot be encrypted cannot carry the HID Service,
		 * and would only keep the profile's host from connecting. */
		(void)bt_conn_disconnect(conn, BT_HCI_ERR_AUTH_FAIL);
		return;
	}

	LOG_INF("Security: %s level %u", bt_conn_dst_str(conn), level);
	if (level >= BT_SECURITY_L2) {
		struct bt_conn_info info;

		ble_ready_callback(true);
		/* Blink the profile's LED: the connection's own identity, as
		 * `active_profile` belongs to `ble_workq`. */
		if (bt_conn_get_info(conn, &info) == 0) {
			ble_connected_callback(info.id);
		}
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
	LOG_WRN("Pairing failed %s: %d %s", bt_conn_dst_str(conn), reason,
		bt_security_err_to_str(reason));
}

struct bond_check {
	const bt_addr_le_t *peer;
	bool other;
};

static void check_bond(const struct bt_bond_info *info, void *data)
{
	struct bond_check *check = data;

	if (!bt_addr_le_eq(&info->addr, check->peer)) {
		check->other = true;
	}
}

/* Refuse a pairing on a profile bonded to some other host.  The bonded host
 * pairing again is let through, to replace its own bond. */
static enum bt_security_err pairing_accept(struct bt_conn *conn,
					   const struct bt_conn_pairing_feat *const feat)
{
	ARG_UNUSED(feat);

	struct bt_conn_info info;

	if (bt_conn_get_info(conn, &info) != 0) {
		return BT_SECURITY_ERR_UNSPECIFIED;
	}

	struct bond_check check = {.peer = bt_conn_get_dst(conn), .other = false};

	bt_foreach_bond(info.id, check_bond, &check);
	if (check.other) {
		LOG_WRN("Profile %u is bonded to another host; refusing %s", info.id,
			bt_conn_dst_str(conn));
		return BT_SECURITY_ERR_PAIR_NOT_ALLOWED;
	}
	return BT_SECURITY_ERR_SUCCESS;
}

/* Only `pairing_accept`: with no passkey callbacks the keyboard still has no
 * input or output, so pairing stays Just Works. */
static const struct bt_conn_auth_cb auth_cb = {
	.pairing_accept = pairing_accept,
};

static struct bt_conn_auth_info_cb auth_info_cb = {
	.pairing_complete = pairing_complete,
	.pairing_failed = pairing_failed,
};

/* Make sure every profile's identity exists.  Identities are kept in
 * settings, so this only creates them on the first boot, or after the
 * settings were erased. */
static int ensure_identities(void)
{
	bt_addr_le_t addrs[CONFIG_BT_ID_MAX];
	size_t count = ARRAY_SIZE(addrs);

	bt_id_get(addrs, &count);
	while (count <= PROFILE_COUNT) {
		int id = bt_id_create(NULL, NULL);

		if (id < 0) {
			return id;
		}
		count = id + 1;
	}

	for (uint8_t profile = 1; profile <= PROFILE_COUNT; profile++) {
		log_profile_addr("is", profile);
	}
	return 0;
}

int ble_setup(void)
{
	int err;

	k_work_queue_start(&ble_workq, ble_workq_stack, K_THREAD_STACK_SIZEOF(ble_workq_stack),
			   BLE_WORKQ_PRIO, NULL);
	k_thread_name_set(k_work_queue_thread_get(&ble_workq), "ble_workq");

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

	err = ensure_identities();
	if (err != 0) {
		LOG_ERR("Identity create failed: %d", err);
		return err;
	}

	err = bt_conn_auth_cb_register(&auth_cb);
	if (err != 0) {
		LOG_ERR("Auth callbacks failed: %d", err);
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

	ble_up = true;
	k_work_submit_to_queue(&ble_workq, &advertise_work);
	return 0;
}

/* Ask for a profile, numbered from 1, to become the active one.  The work is
 * done on `ble_workq`; this only says whether there is such a profile. */
int ble_select_profile(uint8_t profile)
{
	if (!ble_up) {
		return -ENODEV;
	}
	if (profile < 1 || profile > PROFILE_COUNT) {
		return -EINVAL;
	}
	atomic_set(&select_target, profile);
	k_work_submit_to_queue(&ble_workq, &select_work);
	return 0;
}

/* Ask for a profile, numbered from 1, to be forgotten.  As for
 * `ble_select_profile`. */
int ble_clear_profile(uint8_t profile)
{
	if (!ble_up) {
		return -ENODEV;
	}
	if (profile < 1 || profile > PROFILE_COUNT) {
		return -EINVAL;
	}
	atomic_or(&clear_mask, BIT(profile));
	k_work_submit_to_queue(&ble_workq, &clear_work);
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
