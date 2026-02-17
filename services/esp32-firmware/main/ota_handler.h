#pragma once
#include <stdint.h>
#include <stdbool.h>

/* OTA register protocol constants — must match gateway firmware/types.rs */
#define REG_FIRMWARE_VERSION  60001
#define REG_OTA_CONTROL       60100
#define REG_FW_SIZE_HIGH      60101
#define REG_FW_SIZE_LOW       60102
#define REG_CRC_HIGH          60103
#define REG_CRC_LOW           60104
#define REG_DATA_WINDOW_START 60105
#define REG_DATA_WINDOW_END   60232

#define OTA_CMD_START   1
#define OTA_CMD_COMMIT  2
#define OTA_CMD_ABORT   3

#define OTA_STATUS_IDLE       0
#define OTA_STATUS_RECEIVING  1
#define OTA_STATUS_VALIDATING 2
#define OTA_STATUS_SUCCESS    3
#define OTA_STATUS_ERROR      4

#define OTA_CHUNK_SIZE 256

/* Initialize the OTA handler state. */
void ota_init(void);

/* Handle a Modbus write to an OTA register. Called from the Modbus slave callback. */
void ota_handle_write(uint16_t reg, uint16_t value);

/* Read an OTA register value. Called from the Modbus slave callback. */
uint16_t ota_read_register(uint16_t reg);

/* Get the current firmware version (for register 60001). */
uint16_t ota_get_firmware_version(void);
