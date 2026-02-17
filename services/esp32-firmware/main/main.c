#include <stdio.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_log.h"
#include "ota_handler.h"

static const char *TAG = "main";

void app_main(void) {
    ESP_LOGI(TAG, "IoT Gateway Device Firmware v%d.%d",
             ota_get_firmware_version() >> 8,
             ota_get_firmware_version() & 0xFF);

    ota_init();

    /* TODO: Initialize Modbus RTU slave with OTA register callbacks. */
    /* TODO: Initialize WiFi + MQTT for telemetry. */

    ESP_LOGI(TAG, "Device ready - waiting for Modbus commands");
    while (1) {
        vTaskDelay(pdMS_TO_TICKS(1000));
    }
}
