/**
 * ESP32 Modbus RTU Slave — Wind Turbine Motor Controller (0x0008)
 *
 * Wiring:
 *   ESP32 D12 → L9110S A1 (forward PWM)
 *   ESP32 D11 → L9110S A2 (reverse PWM)
 *   ESP32 D27 → KY-003 S  (Hall effect sensor, active LOW)
 *   5V → L9110S VCC, KY-003 VCC, ESP32 VIN
 *   GND → L9110S GND, KY-003 GND, ESP32 GND
 *   L9110S OA → Motor RED
 *   L9110S OB → Motor BLACK
 *   Magnet on motor shaft/blade passes KY-003 once per revolution
 *
 * Writable registers (FC06 Write Single Register):
 *   1000  Motor command    → 0=stop, 1=forward, 2=reverse, 3=brake
 *   1001  Motor speed      → PWM 0-255
 *
 * Readable registers (FC03/FC04):
 *   60000  Device ID        → 0x0008
 *   60001  Software Version → 0x0100 (1.00)
 *   200    Motor state      → 0=stopped, 1=forward, 2=reverse, 3=braking
 *   201    Motor speed      → current PWM 0-255
 *   202    Real RPM         → measured by Hall sensor (pulses/min)
 *   203    Direction        → 0=stopped, 1=forward, 2=reverse
 *   204    Bus voltage (mV) → ~5000
 *   205    Motor current(mA)→ estimated from PWM
 *   206    Uptime (seconds)
 *   207    Pulse count      → total Hall sensor pulses since boot
 */

#include <Arduino.h>

#define PIN_A1  13   // forward PWM
#define PIN_A2  14   // reverse PWM
#define PWM_FREQ 1000
#define PWM_RES  8   // 8-bit = 0-255

// --- Hall effect sensor (KY-003) ---
#define PIN_HALL 27  // digital input, active LOW when magnet detected

#define SLAVE_ID 254

// --- Motor state ---
enum MotorCmd : uint8_t { STOP = 0, FORWARD = 1, REVERSE = 2, BRAKE = 3 };
static MotorCmd motorState = STOP;
static uint8_t motorSpeed = 0;       // target PWM 0-255
static uint8_t currentSpeed = 0;     // actual PWM (ramps toward target)

// --- Hall sensor RPM measurement ---
static volatile uint32_t hallPulseCount = 0;
static uint32_t lastPulseCount = 0;
static unsigned long lastRpmCalcTime = 0;
static uint16_t measuredRpm = 0;

// ISR: called on falling edge (magnet passes sensor)
void IRAM_ATTR hallISR() {
  hallPulseCount++;
}

// --- Sparse register map ---
struct RegEntry { uint16_t addr; uint16_t val; };
static RegEntry regMap[32];
static int regCount = 0;

void regSet(uint16_t addr, uint16_t val) {
  for (int i = 0; i < regCount; i++) {
    if (regMap[i].addr == addr) { regMap[i].val = val; return; }
  }
  if (regCount < 32) { regMap[regCount++] = {addr, val}; }
}

uint16_t regGet(uint16_t addr) {
  for (int i = 0; i < regCount; i++) {
    if (regMap[i].addr == addr) return regMap[i].val;
  }
  return 0;
}

// --- Motor control ---
void applyMotor() {
  switch (motorState) {
    case FORWARD:
      ledcWrite(PIN_A1, currentSpeed);
      ledcWrite(PIN_A2, 0);
      break;
    case REVERSE:
      ledcWrite(PIN_A1, 0);
      ledcWrite(PIN_A2, currentSpeed);
      break;
    case BRAKE:
      ledcWrite(PIN_A1, 255);
      ledcWrite(PIN_A2, 255);
      break;
    case STOP:
    default:
      ledcWrite(PIN_A1, 0);
      ledcWrite(PIN_A2, 0);
      break;
  }
}

void handleMotorCommand(uint16_t cmd) {
  switch (cmd) {
    case 1: motorState = FORWARD; break;
    case 2: motorState = REVERSE; break;
    case 3:
      motorState = BRAKE;
      motorSpeed = 0;
      currentSpeed = 0;
      break;
    case 0:
    default:
      motorState = STOP;
      motorSpeed = 0;
      currentSpeed = 0;
      break;
  }
  applyMotor();
}

// --- Update registers + ramp speed + calculate RPM ---
static unsigned long lastUpdate = 0;

void updateRegisters() {
  if (motorState == FORWARD || motorState == REVERSE) {
    if (currentSpeed < motorSpeed) {
      currentSpeed = min((int)currentSpeed + 5, (int)motorSpeed);
    } else if (currentSpeed > motorSpeed) {
      currentSpeed = max((int)currentSpeed - 5, (int)motorSpeed);
    }
  } else {
    currentSpeed = 0;
  }

  applyMotor();

  unsigned long now = millis();
  if (now - lastRpmCalcTime >= 500) {
    uint32_t currentCount = hallPulseCount;
    uint32_t pulses = currentCount - lastPulseCount;
    unsigned long elapsed = now - lastRpmCalcTime;

    uint32_t revolutions = pulses / 2;
    if (elapsed > 0 && revolutions > 0) {
      measuredRpm = (uint16_t)((revolutions * 60000UL) / elapsed);
    } else {
      measuredRpm = 0;
    }

    lastPulseCount = currentCount;
    lastRpmCalcTime = now;
  }

  uint16_t currentMa = (uint16_t)((float)currentSpeed / 255.0 * 200.0);

  // Status registers
  regSet(200, (uint16_t)motorState);
  regSet(201, currentSpeed);
  regSet(202, measuredRpm);                   // REAL RPM from Hall sensor
  regSet(203, motorState == FORWARD ? 1 :
              motorState == REVERSE ? 2 : 0);
  regSet(204, 5000 + random(-50, 50));
  regSet(205, currentMa);
  regSet(206, (uint16_t)(millis() / 1000));
  regSet(207, (uint16_t)(hallPulseCount & 0xFFFF));  // pulse count (lower 16 bits)

  // Fixed identification
  regSet(60000, 0x0008);
  regSet(60001, 0x0100);
}

// --- Modbus RTU ---
uint16_t crc16(const uint8_t* buf, int len) {
  uint16_t crc = 0xFFFF;
  for (int i = 0; i < len; i++) {
    crc ^= buf[i];
    for (int j = 0; j < 8; j++) {
      if (crc & 1) crc = (crc >> 1) ^ 0xA001;
      else crc >>= 1;
    }
  }
  return crc;
}

void sendResponse(const uint8_t* data, int len) {
  Serial.write(data, len);
  Serial.flush();
}

void handleRequest(const uint8_t* frame, int len) {
  if (len < 8) return;
  if (frame[0] != SLAVE_ID) return;

  uint16_t frameCrc = frame[len - 2] | (frame[len - 1] << 8);
  if (crc16(frame, len - 2) != frameCrc) return;

  uint8_t func = frame[1];

  if (func == 0x03 || func == 0x04) {
    uint16_t startAddr = (frame[2] << 8) | frame[3];
    uint16_t quantity  = (frame[4] << 8) | frame[5];
    if (quantity > 125) return;

    uint8_t resp[5 + quantity * 2];
    resp[0] = SLAVE_ID;
    resp[1] = func;
    resp[2] = quantity * 2;

    for (int i = 0; i < quantity; i++) {
      uint16_t val = regGet(startAddr + i);
      resp[3 + i * 2] = val >> 8;
      resp[4 + i * 2] = val & 0xFF;
    }

    int respLen = 3 + quantity * 2;
    uint16_t rc = crc16(resp, respLen);
    resp[respLen] = rc & 0xFF;
    resp[respLen + 1] = rc >> 8;
    sendResponse(resp, respLen + 2);
  }
  else if (func == 0x06) {
    uint16_t addr = (frame[2] << 8) | frame[3];
    uint16_t val  = (frame[4] << 8) | frame[5];

    if (addr == 1000) {
      handleMotorCommand(val);
      regSet(addr, val);
    } else if (addr == 1001) {
      motorSpeed = val > 255 ? 255 : val;
      regSet(addr, motorSpeed);
    } else {
      regSet(addr, val);
    }

    sendResponse(frame, len);
  }
}

// --- Receive buffer ---
static uint8_t rxBuf[256];
static int rxPos = 0;
static unsigned long lastRx = 0;

void setup() {
  ledcAttach(PIN_A1, PWM_FREQ, PWM_RES);
  ledcAttach(PIN_A2, PWM_FREQ, PWM_RES);

  pinMode(PIN_HALL, INPUT_PULLUP);
  attachInterrupt(digitalPinToInterrupt(PIN_HALL), hallISR, CHANGE);

  // USB serial for Modbus RTU
  Serial.begin(9600);

  lastRpmCalcTime = millis();
  updateRegisters();
}

void loop() {
  if (millis() - lastUpdate > 100) {
    updateRegisters();
    lastUpdate = millis();
  }

  while (Serial.available()) {
    rxBuf[rxPos++] = Serial.read();
    lastRx = millis();
    if (rxPos >= sizeof(rxBuf)) rxPos = 0;
  }

  if (rxPos > 0 && millis() - lastRx > 4) {
    handleRequest(rxBuf, rxPos);
    rxPos = 0;
  }
}
