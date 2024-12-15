use can_mcp2515::drivers::{can::*, message::*};
use embedded_can::{blocking::Can, StandardId};
use embedded_hal::digital::{InputPin, OutputPin, *};
use esp32_nimble::{uuid128, BLEAdvertisementData, BLEDevice, NimbleProperties};
use lib::*;
use log::*;
use protocol::{CurrentMeasurement, FixedLogType, MotorSubSystem, VelocityInfo, WriteType};
use rand::seq::SliceRandom; // Import the random functionality
use rand::thread_rng; // Random number generator

use esp_idf_hal::{delay::FreeRtos, sys::esp_task_wdt_reset};
use esp_idf_hal::gpio::InterruptType;
use esp_idf_svc::hal::{
    gpio::{Level, PinDriver, Pull},
    peripherals::Peripherals,   
    prelude::*,
    spi::{config::Config, config::DriverConfig, config::MODE_0, Dma, SpiBusDriver, SpiDriver},
};

use log::{debug, error, info, warn};

fn main() -> anyhow::Result<(), anyhow::Error> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // CAN MODULE
    let periph = Peripherals::take()?;

    info!("Assigning GPIO peripheral pins now!");
    let spi = periph.spi2;

    // PIN mapping for interfacing with the SpiDriver and SpiBusDriver
    let sck = periph.pins.gpio6;
    let sdo = periph.pins.gpio7; // SDO = MOSI
    let sdi = periph.pins.gpio2; // SDI = MISO

    log::info!("Trying to initialize PinDriver for cs and interrupt pin for CAN driver!");

    let mut cs_pin = PinDriver::output(periph.pins.gpio10)?;
    cs_pin.set_level(Level::High)?;
    //cs_pin.set_state(PinState::High)?;

    let mut can_interrupt = PinDriver::input(periph.pins.gpio1)?;
    can_interrupt.set_pull(Pull::Up)?;
    can_interrupt.set_interrupt_type(InterruptType::NegEdge)?;

    //can_interrupt.enable_interrupt()?;
    // -----------------------------------------------------------------

    log::info!("Trying to initialize SpiDriver!");

    let spi_driver = SpiDriver::new(spi, sck, sdo, Some(sdi), &DriverConfig::default())?;

    //const K125: KiloHertz = 125.kHz();
    let bus_config = Config::new()
        //.baudrate(500u32.kHz().into())
        .baudrate(4.MHz().into())
        .data_mode(MODE_0);

    log::info!("Trying to initialize SpiBusDriver!");
    let spi_bus = SpiBusDriver::new(spi_driver, &bus_config)?;

    const CLKEN: bool = false;
    const OSM: bool = false;
    const ABAT: bool = false;
    const MASK_RXN: u16 = 0b1111_1111_1110_0000;
    const FILTER_RX0: u16 = 0x0;
    const FILTER_RX1: u16 = 0x1;
    const DEFAULT_FILTER_MASK: u16 = Mcp2515Settings::DEFAULT_FILTER_MASK;

    let canctrl_settings = SettingsCanCtrl::new(
        OperationTypes::Configuration,
        CLKEN,
        CLKPRE::DIV1,
        ABAT,
        OSM,
    );

    let can_settings = Mcp2515Settings::new(
        canctrl_settings,
        McpClock::MCP8,
        Bitrate::CAN125,
        0u8,
        ReceiveBufferMode::OnlyStandardId,
        AcceptanceFilterMask::new(DEFAULT_FILTER_MASK, DEFAULT_FILTER_MASK),
        AcceptanceFilterMask::new(DEFAULT_FILTER_MASK, DEFAULT_FILTER_MASK),
    );

    log::info!("Initializing MCP2515 Driver!");
    let mut can_driver = Mcp2515Driver::init(spi_bus, cs_pin, can_interrupt, can_settings);
    log::info!("MCP2515 Driver was successfully created!");

    // BLE Module
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
    ble_advertising
        .lock()
        .set_data(
            BLEAdvertisementData::new()
                .name("ESP32-BLE")
                .add_service_uuid(uuid128!("fafafafa-fafa-fafa-fafa-fafafafafafa")),
        )
        .ok()
        .unwrap();
    ble_advertising.lock().start().ok().unwrap();

    server.ble_gatts_show_local();

    // BLE will update every 1 sec for other devices to see
    loop {
        //esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000); // Send message every 1000ms

        if can_driver.interrupt_pin.is_low() {
            info!("GOT INTERRUPT!!!");
            while !can_driver.interrupt_is_cleared(){
                let interrupt_type = can_driver.interrupt_decode().unwrap();
                info!("type: {:?}", interrupt_type);
                if let Some(frame) = can_driver.handle_interrupt(interrupt_type) {

                    let msg_type = protocol::MessageType::try_from(&frame).unwrap();
                    info!("Can: {:?}", msg_type);
                    match msg_type {
                        protocol::MessageType::FixedLog(FixedLogType::BatteryStatus(
                            battery_status,
                        )) => {
                            let bat_stat = battery_status.0 * 100.0;
                            let range_value = bat_stat * 150.0; // we ASSUME 1% of the battery is 150m
                            // update battery status over BLE
                            battery_characteristic
                            .lock()
                            .set_value(format!("{}", bat_stat).as_bytes()) // Send the random value as bytes
                            .notify();

                            info!("BATTERY: {}", bat_stat);
                            // update range based on percentage
                            range_characteristic
                            .lock()
                            .set_value(format!("{}", range_value).as_bytes()) // Send the random value as bytes
                            .notify();

                            info!("RANGE: {}", range_value);
                        }
                        //| protocol::MessageType::Write(WriteType::Motor(motor_subsystem)),
                        protocol::MessageType::Write(WriteType::Motor(MotorSubSystem::Left(left))) => {
                            let speed_value = left;
                            // update speed over BLE
                            speed_characteristic
                            .lock()
                            .set_value(format!("{}", speed_value).as_bytes()) // Send the random value as bytes
                            .notify();

                            info!("SPEED: {}", speed_value);
                        }
                        _ => {}
                    }
                }
                println!("Zoom");
                //unsafe { esp_task_wdt_reset() };
            }
            if can_driver.interrupt_is_cleared() {
                log::info!("All interrupt is cleared!");
            } else {
                let interrupt_type = can_driver.interrupt_decode().unwrap();
                can_driver.handle_interrupt(interrupt_type);
            }
            //can_driver.interrupt_pin.enable_interrupt()?;
        } else {
            info!("Got nothing");
            FreeRtos::delay_ms(100);
        }
    }
}
