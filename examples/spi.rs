use bstr::ByteSlice;
use embedded_hal::spi::*;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::config::Config;
use esp_idf_hal::spi::*;

pub struct CanMessage {
    pub id: u32,
    pub data: [u8; 8], // Example data
}

fn main() -> ! {
    // Setup handler for device peripherals
    let peripherals = Peripherals::take().unwrap();

    // Create handles for SPI pins
    let sclk = peripherals.pins.gpio0;
    let mosi = peripherals.pins.gpio2;
    let cs = peripherals.pins.gpio3;

    // Instantiate SPI Driver
    let spi_drv = SpiDriver::new(
        peripherals.spi2,
        sclk,
        mosi,
        None::<gpio::AnyIOPin>,
        &SpiDriverConfig::new(),
    )
    .unwrap();

    // Configure Parameters for SPI device
    let config = Config::new().baudrate(2.MHz().into()).data_mode(Mode {
        polarity: Polarity::IdleLow,
        phase: Phase::CaptureOnFirstTransition,
    });

    // Instantiate SPI Device Driver and Pass Configuration
    let mut spi = SpiDeviceDriver::new(spi_drv, Some(cs), &config).unwrap();

    // Prepare "dummy_peepo" as bytes
    let data = b"dummy_peepo"; // Converts "dummy_peepo" into a byte slice

    loop {
        // Log the attempt to send "dummy_peepo" every time in the loop
        println!("Attempting to send: {:?}", data.to_str());

        // Send the "dummy_peepo" data over SPI
        match spi.write(data) {
            Ok(_) => {
                // Log success if data is sent successfully
                println!("Successfully sent: {:?}", data.to_str());
            }
            Err(e) => {
                // Log error if something goes wrong
                println!("Error sending data: {:?}", e);
            }
        }

        // Optional: Add a delay to make the terminal output more readable
        FreeRtos::delay_ms(1000_u32); // 1 second delay between each send attempt
    }
}
