use std::time::Duration;

use libusb::{TransferType, DeviceHandle};
use pretty_hex::PrettyHex;

const VENDOR_ID: u16 = 0x0a41;
const PRODUCT_ID: u16 = 0x7003;

const IN_ENDPOINT_ADDR: u8 = 0x81;
const OUT_ENDPOINT_ADDR: u8 = 0x2;

const TIMEOUT: Duration = Duration::from_millis(1000);

// fn wait_until_ready(h: & mut DeviceHandle) {
//     let mut buf = [0; 2];
//     loop {
//         h.write_bulk(OUT_ENDPOINT_ADDR, &[0x00, 0x00], TIMEOUT).unwrap();
//         let read = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
//         for b in &buf[..read] {
//             print!("{b:02X} ");
//         }
//         println!();

//         if read == 2 && buf == [0x15, 0x31] {
//             return;
//         }
//     }
// }

// fn get_slots(h: &mut DeviceHandle) {
//     h.write_bulk(OUT_ENDPOINT_ADDR, b"GT0001", TIMEOUT).unwrap();

//     let mut buf = [0; 2048];
//     let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
//     println!("{:?}", buf[..len].hex_dump());
//     let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
//     println!("{:?}", buf[..len].hex_dump());
// }

fn startup(h: &mut DeviceHandle) {
    h.write_bulk(OUT_ENDPOINT_ADDR, b"ST", TIMEOUT).unwrap();

    let mut buf = [0; 2048];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
}

fn rt0(h: &mut DeviceHandle) {
    h.write_bulk(OUT_ENDPOINT_ADDR, b"RT0", TIMEOUT).unwrap();

    let mut buf = [0; 2048];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
}

fn rt1(h: &mut DeviceHandle) {
    h.write_bulk(OUT_ENDPOINT_ADDR, b"RT1", TIMEOUT).unwrap();

    let mut buf = [0; 2048];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
}

fn mn(h: &mut DeviceHandle) {
    h.write_bulk(OUT_ENDPOINT_ADDR, b"MN", TIMEOUT).unwrap();

    let mut buf = [0; 2048];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
}

fn sar(h: &mut DeviceHandle) {
    h.write_bulk(OUT_ENDPOINT_ADDR, b"S", TIMEOUT).unwrap();

    let mut buf = [0; 2048];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
}

fn make_req(h: &mut DeviceHandle, req: &[u8]) {
    println!("REQ: {:?}", std::str::from_utf8(req).unwrap());
    h.write_bulk(OUT_ENDPOINT_ADDR, req, TIMEOUT).unwrap();

    let mut buf = [0; 8192];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
    println!("{:?}", buf[..len].hex_dump());
}


// fn get_slots(h: &mut DeviceHandle) {
//     do_req(h, b"GT0001");
// }

// fn do_req(h: &mut DeviceHandle, req: &[u8]) -> Vec<Vec<u8>> {
//     h.write_bulk(OUT_ENDPOINT_ADDR, req, TIMEOUT).unwrap();

//     let mut buf = vec![0; 2048];
//     loop {
//         let read = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
//         if read == 2 && &buf[0..2] == MORE_RET {
//             continue;
//         }

//     }
// }

fn main() {
    let ctx = libusb::Context::new().unwrap();
    let devs = ctx.devices().unwrap();

    let d = devs
        .iter()
        .find(|d| {
            let desc = d.device_descriptor().unwrap();
            desc.vendor_id() == VENDOR_ID && desc.product_id() == PRODUCT_ID
        })
        .expect("No sekonic 7000 dectected");

    let desc = d.device_descriptor().unwrap();
    let mut h = d.open().unwrap();

    let mut out_endpoint = None;
    let mut in_endpoint = None;
    'outer: for n in 0..desc.num_configurations() {
        let config_desc = match d.config_descriptor(n) {
            Ok(c) => c,
            Err(_) => continue
        };

        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                out_endpoint = None;
                in_endpoint = None;
                for endpoint_desc in interface_desc.endpoint_descriptors() {
                    if endpoint_desc.direction() == libusb::Direction::Out && endpoint_desc.transfer_type() == TransferType::Bulk {
                        println!("found OUT endpoint number={} config={} iface={} setting={} address={}", endpoint_desc.number(), config_desc.number(), interface_desc.interface_number(), interface_desc.setting_number() , endpoint_desc.address());
                        out_endpoint = Some(endpoint_desc.address());
                    }
                    if endpoint_desc.direction() == libusb::Direction::In && endpoint_desc.transfer_type() == TransferType::Bulk {
                        println!("found IN endpoint number={} config={} iface={} setting={} address={}", endpoint_desc.number(), config_desc.number(), interface_desc.interface_number(), interface_desc.setting_number() , endpoint_desc.address());
                        in_endpoint = Some(endpoint_desc.address());
                    }
                }
                if let (Some(out), Some(i)) = (out_endpoint, in_endpoint) {
                        h.set_active_configuration(config_desc.number()).unwrap();
                        h.claim_interface(interface_desc.interface_number()).unwrap();
                        // h.set_alternate_setting(interface_desc.interface_number(), interface_desc.setting_number()).unwrap();

                        break 'outer;
                }
            }
        }
    }
    // let out_endpoint = out_endpoint.unwrap();
    // let in_endpoint = in_endpoint.unwrap();
    // wait_until_ready(&mut h);
    // get_slots(&mut h);

    make_req(&mut h, b"ST");
    make_req(&mut h, b"RT0");
    make_req(&mut h, b"RT1");
    make_req(&mut h, b"MN");
    make_req(&mut h, b"SAr");
    make_req(&mut h, b"FTr");
    make_req(&mut h, b"FV");
    make_req(&mut h, b"IUr");
    make_req(&mut h, b"MI");
    make_req(&mut h, b"GT0001");
    make_req(&mut h, b"GA0001,0001");
    make_req(&mut h, b"MR0001");
    make_req(&mut h, b"GA0001,0002");
    make_req(&mut h, b"MR0002");
    make_req(&mut h, b"GA0001,0003");
    make_req(&mut h, b"MR0003");
    make_req(&mut h, b"GA0001,0004");
    make_req(&mut h, b"MR0004");
    make_req(&mut h, b"ME0004");
}
