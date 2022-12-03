#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use defmt::{panic, *};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, Instance};
use embassy_stm32::{interrupt, Config};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.hse = Some(Hertz(8_000_000));
    config.rcc.sys_ck = Some(Hertz(48_000_000));
    config.rcc.pclk1 = Some(Hertz(24_000_000));
    let mut p = embassy_stm32::init(config);

    info!("Hello World!");

    {
        let _dp = Output::new(&mut p.PA12, Level::Low, Speed::Low);
        Timer::after(Duration::from_millis(10)).await;
    }

    let irq = interrupt::take!(USB_LP_CAN1_RX0);
    let driver = Driver::new(p.USB, irq, p.PA12, p.PA11);

    let config = embassy_usb::Config::new(0xc0de, 0xcafe);
    let mut device_descriptor = [0; 256];
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 7];

    let mut state = State::new();

    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        &mut device_descriptor,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut control_buf,
        None,
    );

    let mut class = CdcAcmClass::new(&mut builder, &mut state, 64);
    let mut usb = builder.build();
    let usb_fut = usb.run();

    let process_fut = async {
        loop {
            class.wait_connection().await;
            info!("Connected");
            let _ = process(&mut class).await;
            info!("Disconnected");
        }
    };

    join(usb_fut, process_fut).await;
}

struct Disconnected;

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn process<'d, T: Instance + 'd>(
    class: &mut CdcAcmClass<'d, Driver<'d, T>>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    let mut total_max_sums: [u32; 3] = [0, 0, 0];
    let mut current_sum: u32 = 0;
    let mut current_num: u32 = 0;

    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];

        for x in data {
            if *x == 0 {
                let mut result: [u8; 4] = [0; 4];
                result[3] = (total_max_sums[0] >> 24) as u8;
                result[2] = (total_max_sums[0] >> 16 & 0xff) as u8;
                result[1] = (total_max_sums[0] >> 8 & 0xff) as u8;
                result[0] = (total_max_sums[0] >> 0 & 0xff) as u8;
                class.write_packet(&result).await?;

                let total = total_max_sums[0] + total_max_sums[1] + total_max_sums[2];
                result[3] = (total >> 24) as u8;
                result[2] = (total >> 16 & 0xff) as u8;
                result[1] = (total >> 8 & 0xff) as u8;
                result[0] = (total >> 0 & 0xff) as u8;
                class.write_packet(&result).await?;
            } else if *x >= 48 && *x <= 57 {
                current_num = current_num * 10 + (*x as u32) - 48;
            } else if *x == 0x0a {
                if current_num > 0 {
                    current_sum += current_num;
                    current_num = 0;
                } else {
                    if current_sum > total_max_sums[0] {
                        total_max_sums[2] = total_max_sums[1];
                        total_max_sums[1] = total_max_sums[0];
                        total_max_sums[0] = current_sum;
                    }

                    current_sum = 0;
                }
            }
        }
    }
}
