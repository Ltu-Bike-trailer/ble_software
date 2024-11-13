use esp32_nimble::{uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties};
use log::*;
use rand::seq::SliceRandom; // Import the random functionality
use rand::thread_rng; // Random number generator

fn main() {
  esp_idf_svc::sys::link_patches();
  esp_idf_svc::log::EspLogger::initialize_default();

  info!("Control over BLE");
  let ble_device = BLEDevice::take(); // Here we take control over the BLE unit on the ESP32C3
  let ble_advertising = ble_device.get_advertising();

  let server = ble_device.get_server();
  server.on_connect(|server, desc| {
    ::log::info!("Client connected: {:?}", desc);

    server
      .update_conn_params(desc.conn_handle(), 24, 48, 0, 60)
      .unwrap();

    if server.connected_count() < (esp_idf_svc::sys::CONFIG_BT_NIMBLE_MAX_CONNECTIONS as _) {
      ::log::info!("Multi-connect support: start advertising"); // In-case we want several phones etc. to be able to connect to same device/cart
      ble_advertising.lock().start().unwrap();
    }
  });

  server.on_disconnect(|_desc, reason| {
    ::log::info!("Client disconnected ({:?})", reason);
  });
  
  info!("Starting service");
  let service = server.create_service(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa")); // A unique identifier for our service (cart ID in this case)

  // [SPEED] characteristic that notifies the App when there's a change in speed
  let speed_characteristic = service.lock().create_characteristic(
    uuid128!("a3c87500-8ed3-4bdf-8a39-a01bebede295"), // Unique service ID which we will use for App to read SPEED value from
    NimbleProperties::READ | NimbleProperties::NOTIFY,
  );

  // [RANGE] characteristic that notifies the App when there's a change
  let range_characteristic = service.lock().create_characteristic(
    uuid128!("4f548a6e-3e95-4afe-92b0-b0d9b32fb04a"), // Unique service ID which we will use for App to read RANGE value from
    NimbleProperties::READ | NimbleProperties::NOTIFY,
  );

  // [BATTERY] characteristic that notifies the App when there's a changes
  let battery_characteristic = service.lock().create_characteristic(
    uuid128!("c94f81b6-7240-401b-8641-b09e746352dc"), // Unique service ID which we will use for App to read BATTERY value from
    NimbleProperties::READ | NimbleProperties::NOTIFY,
  );

  info!("advertising started"); // Here we start a new advertising session so that others can find and connect to "ESP32-BLE", which is the name we chose for testing
  ble_advertising.lock().set_data(
    BLEAdvertisementData::new()
      .name("ESP32-BLE")
      .add_service_uuid(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa")),
  ).ok().unwrap();
  ble_advertising.lock().start().ok().unwrap();

  server.ble_gatts_show_local();

  let speed = [0u8, 1u8, 2u8, 3u8, 4u8, 5u8]; // Array of unsigned 8-bit integers (later on we will have message IDs and values attached to them over CAN module)
  let range = [100u8, 150u8]; // Array of unsigned 8-bit integers for testing range ^
  let battery = [50u8, 95u8, 80u8, 30u8]; // Array of unsigned 8-bit integers for testing range 
  // BLE will update every 1 sec for other devices to see
  loop {
    esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000); // Send message every 1000ms

    // Randomly select a value from the array `vs`
    let mut rng = thread_rng(); // Create a random number generator
    let speed_rng_value = speed.choose(&mut rng).unwrap(); // Pick a random value from predefined list
    let range_rng_value = range.choose(&mut rng).unwrap(); // Pick a random value from predefined list
    let battery_rng_value = battery.choose(&mut rng).unwrap(); // Pick a random value from predefined list

    // Send the selected random value as a notification to the connected phone.
    speed_characteristic
        .lock()
        .set_value(format!("{}", speed_rng_value).as_bytes()) // Send the random value as bytes
        .notify();

    // Speed value sent, for debugging
    info!("SPEED: {}", speed_rng_value);

    // Send the selected random value as a notification to the connected phone.
    range_characteristic
        .lock()
        .set_value(format!("{}", range_rng_value).as_bytes()) // Send the random value as bytes
        .notify();

    // Log the value sent for debugging
    info!("RANGE: {}", range_rng_value);

    // Send the selected random value as a notification to the connected phone.
    battery_characteristic
        .lock()
        .set_value(format!("{}", battery_rng_value).as_bytes()) // Send the random value as bytes
        .notify();

    // Log the value sent for debugging
    info!("BATTERY: {}", battery_rng_value);
  }
}
