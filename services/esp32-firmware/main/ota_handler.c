#include "ota_handler.h"
#include "esp_ota_ops.h"
#include "esp_log.h"
#include <string.h>

static const char *TAG = "ota";

static struct {
    bool receiving;
    uint8_t *buf;
    uint32_t buf_len;
    uint32_t expected_size;
    uint32_t expected_crc;
    uint16_t status;
    uint16_t firmware_version;
    uint16_t data_window[128];
} ota_state;

/* CRC32 - same algorithm as gateway. */
static uint32_t crc32(const uint8_t *data, size_t len) {
    uint32_t crc = 0xFFFFFFFF;
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int j = 0; j < 8; j++) {
            crc = (crc & 1) ? (crc >> 1) ^ 0xEDB88320 : crc >> 1;
        }
    }
    return ~crc;
}

void ota_init(void) {
    memset(&ota_state, 0, sizeof(ota_state));
    ota_state.status = OTA_STATUS_IDLE;
    /* Read version from app descriptor. */
    const esp_app_desc_t *desc = esp_app_get_description();
    /* Parse "1.0" → 0x0100 from version string. */
    int major = 1, minor = 0;
    sscanf(desc->version, "%d.%d", &major, &minor);
    ota_state.firmware_version = (major << 8) | minor;
    ESP_LOGI(TAG, "Firmware version: %d.%d (0x%04X)",
             major, minor, ota_state.firmware_version);
}

void ota_handle_write(uint16_t reg, uint16_t value) {
    if (reg == REG_OTA_CONTROL) {
        switch (value) {
        case OTA_CMD_START:
            ESP_LOGI(TAG, "OTA START");
            free(ota_state.buf);
            ota_state.buf = malloc(4 * 1024 * 1024); /* 4MB max */
            ota_state.buf_len = 0;
            ota_state.receiving = true;
            ota_state.status = OTA_STATUS_RECEIVING;
            break;
        case OTA_CMD_COMMIT: {
            ESP_LOGI(TAG, "OTA COMMIT (size=%lu, expected=%lu)",
                     ota_state.buf_len, ota_state.expected_size);
            ota_state.status = OTA_STATUS_VALIDATING;
            ota_state.receiving = false;
            /* Validate. */
            uint32_t actual_crc = crc32(ota_state.buf, ota_state.buf_len);
            if (actual_crc == ota_state.expected_crc &&
                ota_state.buf_len == ota_state.expected_size) {
                /* Write to OTA partition. */
                const esp_partition_t *part = esp_ota_get_next_update_partition(NULL);
                esp_ota_handle_t handle;
                if (esp_ota_begin(part, ota_state.buf_len, &handle) == ESP_OK &&
                    esp_ota_write(handle, ota_state.buf, ota_state.buf_len) == ESP_OK &&
                    esp_ota_end(handle) == ESP_OK &&
                    esp_ota_set_boot_partition(part) == ESP_OK) {
                    ota_state.status = OTA_STATUS_SUCCESS;
                    ESP_LOGI(TAG, "OTA flash successful - rebooting in 2s");
                    vTaskDelay(pdMS_TO_TICKS(2000));
                    esp_restart();
                } else {
                    ota_state.status = OTA_STATUS_ERROR;
                    ESP_LOGE(TAG, "OTA flash failed");
                }
            } else {
                ota_state.status = OTA_STATUS_ERROR;
                ESP_LOGE(TAG, "CRC mismatch: got 0x%08lX, expected 0x%08lX",
                         actual_crc, ota_state.expected_crc);
            }
            free(ota_state.buf);
            ota_state.buf = NULL;
            break;
        }
        case OTA_CMD_ABORT:
            ESP_LOGW(TAG, "OTA ABORT");
            ota_state.receiving = false;
            ota_state.status = OTA_STATUS_IDLE;
            free(ota_state.buf);
            ota_state.buf = NULL;
            break;
        default:
            /* Chunk-ready signal (0x10 + index). */
            if (value >= 0x10 && ota_state.receiving) {
                /* Copy data window to firmware buffer. */
                for (int i = 0; i < 128 && ota_state.buf_len < ota_state.expected_size; i++) {
                    ota_state.buf[ota_state.buf_len++] = ota_state.data_window[i] >> 8;
                    if (ota_state.buf_len < ota_state.expected_size) {
                        ota_state.buf[ota_state.buf_len++] = ota_state.data_window[i] & 0xFF;
                    }
                }
                ota_state.status = OTA_STATUS_RECEIVING;
            }
            break;
        }
    } else if (reg == REG_FW_SIZE_HIGH) {
        ota_state.expected_size = (ota_state.expected_size & 0xFFFF) | ((uint32_t)value << 16);
    } else if (reg == REG_FW_SIZE_LOW) {
        ota_state.expected_size = (ota_state.expected_size & 0xFFFF0000) | value;
    } else if (reg == REG_CRC_HIGH) {
        ota_state.expected_crc = (ota_state.expected_crc & 0xFFFF) | ((uint32_t)value << 16);
    } else if (reg == REG_CRC_LOW) {
        ota_state.expected_crc = (ota_state.expected_crc & 0xFFFF0000) | value;
    } else if (reg >= REG_DATA_WINDOW_START && reg <= REG_DATA_WINDOW_END) {
        ota_state.data_window[reg - REG_DATA_WINDOW_START] = value;
    }
}

uint16_t ota_read_register(uint16_t reg) {
    if (reg == REG_OTA_CONTROL) return ota_state.status;
    if (reg == REG_FIRMWARE_VERSION) return ota_state.firmware_version;
    return 0;
}

uint16_t ota_get_firmware_version(void) {
    return ota_state.firmware_version;
}
