use std::{
    array,
    cmp::min,
    collections::BTreeMap,
    fmt,
    fs::File,
    io::{stdin, Write},
    path::Path,
    str,
    time::Duration,
};

use anyhow::{bail, format_err};
use libusb::{DeviceHandle, TransferType};
use pretty_hex::PrettyHex;

const VENDOR_ID: u16 = 0x0a41;
const PRODUCT_ID: u16 = 0x7003;

const IN_ENDPOINT_ADDR: u8 = 0x81;
const OUT_ENDPOINT_ADDR: u8 = 0x2;

const TIMEOUT: Duration = Duration::from_millis(1000);

const RESP_OK: [u8; 2] = [0x6, 0x30];
const RESP_BADREQ: [u8; 2] = [0x15, 0x32];

struct HVec(Vec<u8>);

impl fmt::Debug for HVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0.hex_dump())
    }
}

impl From<Vec<u8>> for HVec {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

fn make_req(h: &mut DeviceHandle, req: &[u8]) -> Vec<u8> {
    // println!("REQ: {:?}", std::str::from_utf8(req).unwrap());
    h.write_bulk(OUT_ENDPOINT_ADDR, req, TIMEOUT).unwrap();

    let mut buf = [0; 8192];
    let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();

    if len != 2 {
        println!("expected 2 bytes from first bulk in, strange");
        println!("{:?}", buf[..len].hex_dump());
        panic!();
    }
    let res = [buf[0], buf[1]];
    match res {
        RESP_OK => {
            let len = h.read_bulk(IN_ENDPOINT_ADDR, &mut buf, TIMEOUT).unwrap();
            // println!("{:?}", buf[..len].hex_dump());
            Vec::from(&buf[..len])
        }
        RESP_BADREQ => {
            panic!("bad reqeust")
        }
        _ => {
            panic!("unknown response {:?}", res.hex_dump());
        }
    }
}

struct ParseHelper<'a> {
    remaining: &'a [u8],
}

impl<'a> ParseHelper<'a> {
    fn start(to_parse: &'a [u8], name: &str) -> Option<ParseHelper<'a>> {
        if !to_parse.starts_with(name.as_bytes()) {
            println!("unpexected start");
            return None;
        }

        if to_parse[name.len()..name.len() + 2] != b"@@"[..] {
            return None;
        }

        Some(ParseHelper {
            remaining: &to_parse[name.len() + 2..],
        })
    }

    fn bytes(&mut self) -> &'a [u8] {
        let len = self
            .remaining
            .iter()
            .position(|b| *b == b',')
            .unwrap_or(self.remaining.len());
        let ret = &self.remaining[..len];
        self.remaining = &self.remaining[min(self.remaining.len(), len + 1)..];
        ret
    }

    fn bytes_exact(&mut self, len: usize) -> anyhow::Result<&'a [u8]> {
        if len > self.remaining.len() || (len < self.remaining.len() && self.remaining[len] != b',')
        {
            bail!("did not find a ',' in the right distance")
        }
        let ret = &self.remaining[..len];
        self.remaining = &self.remaining[min(self.remaining.len(), len + 1)..];
        Ok(ret)
    }

    fn unsigned(&mut self) -> Option<u32> {
        str::from_utf8(self.bytes()).ok()?.parse().ok()
    }

    fn string(&mut self) -> Option<String> {
        let str = str::from_utf8(self.bytes()).ok()?;
        Some(
            if let Some(idx) = str.find('\0') {
                &str[..idx]
            } else {
                str
            }
            .to_owned(),
        )
    }

    fn float(&mut self) -> anyhow::Result<f32> {
        let b = self.bytes_exact(4)?;
        Ok(f32::from_be_bytes(b.try_into().map_err(|e| {
            format_err!("wrong length, expected 4 got {}", b.len())
        })?))
    }

    fn double(&mut self) -> anyhow::Result<f64> {
        let b = self.bytes_exact(8)?;
        Ok(f64::from_be_bytes(b.try_into().map_err(|e| {
            format_err!("wrong length, expected 8 got {}", b.len())
        })?))
    }

    fn collect_remaining(&mut self) -> Vec<HVec> {
        let mut ret = vec![];
        loop {
            let b = self.bytes();
            if b.len() == 0 {
                return ret;
            }

            ret.push(b.to_owned().into())
        }
    }

    fn float_array<const LEN: usize>(&mut self) -> anyhow::Result<[f32; LEN]> {
        let b = self.bytes_exact(4 * LEN)?;
        Ok(array::from_fn(|i| {
            f32::from_be_bytes([b[i * 4 + 0], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]])
        }))
    }
}

// "MIB" structure
#[derive(Debug)]
struct StorageInfoResp {
    _unk1: u32,
    num_captures: u32,
    num_titles: u32,
}

impl StorageInfoResp {
    fn parse(i: &[u8]) -> StorageInfoResp {
        let mut p = ParseHelper::start(i, "MIB").unwrap();
        StorageInfoResp {
            _unk1: p.unsigned().unwrap(),
            num_captures: p.unsigned().unwrap(),
            num_titles: p.unsigned().unwrap(),
        }
    }
}

fn get_storage_info(d: &mut DeviceHandle) -> StorageInfoResp {
    StorageInfoResp::parse(&make_req(d, b"MI"))
}

// "GTB" structure
#[derive(Debug)]
struct TitleInfo {
    name: String,
    num_captures: u32,
}

impl TitleInfo {
    fn parse(i: &[u8]) -> TitleInfo {
        let mut p = ParseHelper::start(i, "GTB").unwrap();
        TitleInfo {
            name: p.string().unwrap(),
            num_captures: p.unsigned().unwrap(),
        }
    }
}

// 1 indexed
fn get_title_info(d: &mut DeviceHandle, id: u32) -> TitleInfo {
    assert!(id > 0);
    TitleInfo::parse(&make_req(d, format!("GT{id:04}").as_bytes()))
}

// 1 indexed
fn get_global_capture_id(d: &mut DeviceHandle, title_id: u32, local_capture_id: u32) -> u32 {
    assert!(title_id > 0);
    assert!(local_capture_id > 0);

    ParseHelper::start(
        &make_req(
            d,
            format!("GA{title_id:04},{local_capture_id:04}").as_bytes(),
        ),
        "GAB",
    )
    .unwrap()
    .unsigned()
    .unwrap()
}

// "MRB" structure
#[derive(Debug)]
struct CaptureInfo {
    unk0: u32,
    title: String, // NOTE: not title of capture, title of "title", lol
    unk1: u32,     // 6
    unk2: u32,     // 0
    unk3: u32,     // 00
    unk4: u32,     // 0
    unk5: HVec,    // all null
    unk6: u32,     // 0
    unk7: HVec,    // all null
    unk8: u32,     // 0
    cct_k: f32,
    uv_angle: f32, // unsure what to call this lol. output has "⊿uv"
    unk11: u32,    // 0
    unks: [HVec; 6],
    illum_lx: f32,
    illum_fc: f32,
    tristimulus_x: f64,
    tristimulus_y: f64,
    tristimulus_z: f64,
    cie1931_x: f32,
    cie1931_y: f32,
    // cie1931_z: f32, ?????
    cie1976_up: f32,
    unk12: f32,
    unk13: f32,
    cie1976_vp: f32,
    dominant_wavelength: f32,
    purity: f32,
    // ppfd: f32,
    cri_ra: f32,
    cri: [f32; 15],

    // 5nm steps starting at 380nm
    spectral_data_5nm: [f32; 81],

    // 1nm steps starting at 380nm
    spectral_data_1nm: [f32; 401],
    unk14: [u32; 4],
    unk15: [f32; 2],
    ppfd: f32,

    // tm_30_rf: f32,
    // tm_30_rg: f32,
    // ssit: f32,
    // ssid: f32,
    // ssi1: f32,
    // ssi2: f32,
    // tlci: f32,
    // tlmf: f32,
    // and so many more...
    remaining: Vec<HVec>,
}

impl CaptureInfo {
    fn parse(i: &[u8]) -> CaptureInfo {
        let mut p = ParseHelper::start(i, "MRB").unwrap();
        CaptureInfo {
            unk0: p.unsigned().unwrap(),
            title: p.string().unwrap(),
            unk1: p.unsigned().unwrap(),
            unk2: p.unsigned().unwrap(),
            unk3: p.unsigned().unwrap(),
            unk4: p.unsigned().unwrap(),
            unk5: p.bytes().to_owned().into(),
            unk6: p.unsigned().unwrap(),
            unk7: p.bytes().to_owned().into(),
            unk8: p.unsigned().unwrap(),
            cct_k: p.float().unwrap(),
            uv_angle: p.float().unwrap(),
            unk11: p.unsigned().unwrap(),
            unks: array::from_fn(|_| p.bytes().to_owned().into()),
            illum_lx: p.float().unwrap(),
            illum_fc: p.float().unwrap(),
            tristimulus_x: p.double().unwrap(),
            tristimulus_y: p.double().unwrap(),
            tristimulus_z: p.double().unwrap(),
            cie1931_x: p.float().unwrap(),
            cie1931_y: p.float().unwrap(),
            // cie1931_z: p.float().unwrap(),
            cie1976_up: p.float().unwrap(),
            unk12: p.float().unwrap(),
            unk13: p.float().unwrap(),
            cie1976_vp: p.float().unwrap(),
            dominant_wavelength: p.float().unwrap(),
            purity: p.float().unwrap(),
            // ppfd: p.float().unwrap(),
            cri_ra: p.float().unwrap(),
            cri: array::from_fn(|_| p.float().unwrap()),
            spectral_data_5nm: p.float_array().unwrap(),
            spectral_data_1nm: p.float_array().unwrap(),
            // tm_30_rf: p.float().unwrap(),
            // tm_30_rg: p.float().unwrap(),
            // ssit: p.float().unwrap(),
            // ssid: p.float().unwrap(),
            // ssi1: p.float().unwrap(),
            // ssi2: p.float().unwrap(),
            // tlci: p.float().unwrap(),
            // tlmf: p.float().unwrap(),
            unk14: array::from_fn(|_| p.unsigned().unwrap()),
            unk15: array::from_fn(|_| p.float().unwrap()),
            ppfd: p.float().unwrap(),
            remaining: p.collect_remaining(),
        }
    }
}

fn get_capture_info(d: &mut DeviceHandle, global_capture_id: u32) -> CaptureInfo {
    CaptureInfo::parse(&make_req(d, format!("MR{global_capture_id:04}").as_bytes()))
}

// Probably need to name this better, oh well
// "MEB" structure
#[derive(Debug)]
struct CaptureData {
    tm_30_rf: f32,
    tm_30_rg: f32,
    illuminants: [[f32; 4]; 16],
    ssit: f32,
    ssid: f32,
    unk3: u32,
    unk4: f32,
    unk5: u32,
    unk6: f32,
    tlci: f32,
    unk8: u32,
    unk9: [f32; 3],
    unk10: u32,
    unk11: u32,
    // unk2: [f32; 10],
    // remaining: HVec,
}

impl CaptureData {
    fn parse(i: &[u8]) -> CaptureData {
        let mut p = ParseHelper::start(i, "MEB").unwrap();
        let tm_30_rf = p.float().unwrap();
        let tm_30_rg = p.float().unwrap();
        let mut illuminants = [[0.; 4]; 16];
        for row in 0..16 {
            for col in 0..4 {
                illuminants[row][col] = p.float().unwrap();
            }
        }
        // let mut unk2 = [0.; 10];
        // for u in &mut unk2 {
        //     *u = p.float().unwrap();
        // }
        CaptureData {
            tm_30_rf,
            tm_30_rg,
            illuminants,
            ssit: p.float().unwrap(),
            ssid: p.float().unwrap(),
            unk3: p.unsigned().unwrap(),
            unk4: p.float().unwrap(),
            unk5: p.unsigned().unwrap(),
            unk6: p.float().unwrap(),
            tlci: p.float().unwrap(),
            unk8: p.unsigned().unwrap(),
            unk9: array::from_fn(|_| p.float().unwrap()),
            unk10: p.unsigned().unwrap(),
            unk11: p.unsigned().unwrap(),
            // remaining: p.remaining.to_owned().into(),
        }
    }
}

fn get_capture_data(d: &mut DeviceHandle, global_capture_id: u32) -> CaptureData {
    CaptureData::parse(&make_req(d, format!("ME{global_capture_id:04}").as_bytes()))
}

fn write_csv(cd: &CaptureData, ci: &CaptureInfo, local_capture_idx: u32, path: &Path) {
    let mut f = File::create(path).unwrap();
    writeln!(
        &mut f,
        "Date Saved,{}",
        chrono::offset::Local::now().format("%Y/%m/%d %H:%M:%S")
    )
    .unwrap();
    writeln!(
        &mut f,
        "Title,{}_{:03}_{:02}°_{:.0}K\n",
        ci.title, local_capture_idx, 2, ci.cct_k
    )
    .unwrap(); // TODO: angle
               // writeln!(&mut f, "Measuring Mode,{}", 999).unwrap(); // TODO:
               // writeln!(&mut f, "Viewing Angle,{}", 999).unwrap(); // TODO:
    writeln!(&mut f, "CCT [K],{:.0}", ci.cct_k).unwrap();
    writeln!(&mut f, "⊿uv,{:.4}", ci.uv_angle).unwrap();
    writeln!(&mut f, "Illuminance [lx],{:.0}", ci.illum_lx).unwrap();
    writeln!(&mut f, "Illuminance [fc],{:.1}", ci.illum_fc).unwrap();
    writeln!(
        &mut f,
        "Peak Wavelength [nm],{}",
        ci.spectral_data_1nm
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap()
            .0
            + 380
    )
    .unwrap(); // TODO
    writeln!(&mut f, "Tristimulus Value X,{:.4}", ci.tristimulus_x).unwrap();
    writeln!(&mut f, "Tristimulus Value Y,{:.4}", ci.tristimulus_y).unwrap();
    writeln!(&mut f, "Tristimulus Value Z,{:.4}", ci.tristimulus_z).unwrap();
    writeln!(&mut f, "CIE1931 x,{:.4}", ci.cie1931_x).unwrap();
    writeln!(&mut f, "CIE1931 y,{:.4}", ci.cie1931_y).unwrap();
    writeln!(&mut f, "CIE1931 z,{:.4}", 1. - ci.cie1931_x - ci.cie1931_y).unwrap();
    writeln!(&mut f, "CIE1976 u',{:.4}", ci.cie1976_up).unwrap();
    writeln!(&mut f, "CIE1976 v',{:.4}", ci.cie1976_vp).unwrap();
    writeln!(
        &mut f,
        "Dominant Wavelength [nm],{:.0}",
        ci.dominant_wavelength
    )
    .unwrap();
    writeln!(&mut f, "Purity [%],{:.1}", ci.purity).unwrap();
    writeln!(&mut f, "PPFD [umolm⁻²s⁻¹],{:.1}", ci.ppfd).unwrap();
    writeln!(&mut f, "CRI Ra,{:.1}", ci.cri_ra).unwrap();
    for (i, val) in ci.cri.iter().enumerate() {
        writeln!(&mut f, "CRI R{},{:.1}", i + 1, val).unwrap();
    }
    writeln!(&mut f, "TM-30 Rf,{:.0}", cd.tm_30_rf).unwrap();
    writeln!(&mut f, "TM-30 Rg,{:.0}", cd.tm_30_rg).unwrap();
    writeln!(&mut f, "SSIt,{:.0}", cd.ssit).unwrap();
    writeln!(&mut f, "SSId,{:.0}", cd.ssid).unwrap();
    writeln!(&mut f, "TLCI,{:.0}", cd.tlci).unwrap();
    // TODO: a few fields belong here
    writeln!(&mut f, "").unwrap();
    for (i, val) in ci.spectral_data_5nm.iter().enumerate() {
        writeln!(&mut f, "Spectral Data {}[nm],{:.12}", 380 + i * 5, val).unwrap();
    }
    writeln!(&mut f, "").unwrap();
    for (i, val) in ci.spectral_data_1nm.iter().enumerate() {
        writeln!(&mut f, "Spectral Data {}[nm],{:.12}", 380 + i, val).unwrap();
    }
    writeln!(&mut f, "").unwrap();
    writeln!(&mut f, "TM-30 Color Vector Graphic,Reference Illuminant x,Reference Illuminant y,Measured Illuminant x,Measured Illuminant y").unwrap();
    for (i, val) in cd.illuminants.iter().enumerate() {
        writeln!(
            &mut f,
            "bin{},{:.7},{:.7},{:.7},{:.7}",
            i + 1,
            val[0],
            val[1],
            val[2],
            val[3]
        )
        .unwrap();
    }
}

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
            Err(_) => continue,
        };

        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                out_endpoint = None;
                in_endpoint = None;
                for endpoint_desc in interface_desc.endpoint_descriptors() {
                    if endpoint_desc.direction() == libusb::Direction::Out
                        && endpoint_desc.transfer_type() == TransferType::Bulk
                    {
                        println!(
                            "found OUT endpoint number={} config={} iface={} setting={} address={}",
                            endpoint_desc.number(),
                            config_desc.number(),
                            interface_desc.interface_number(),
                            interface_desc.setting_number(),
                            endpoint_desc.address()
                        );
                        out_endpoint = Some(endpoint_desc.address());
                    }
                    if endpoint_desc.direction() == libusb::Direction::In
                        && endpoint_desc.transfer_type() == TransferType::Bulk
                    {
                        println!(
                            "found IN endpoint number={} config={} iface={} setting={} address={}",
                            endpoint_desc.number(),
                            config_desc.number(),
                            interface_desc.interface_number(),
                            interface_desc.setting_number(),
                            endpoint_desc.address()
                        );
                        in_endpoint = Some(endpoint_desc.address());
                    }
                }
                if let (Some(out), Some(i)) = (out_endpoint, in_endpoint) {
                    h.set_active_configuration(config_desc.number()).unwrap();
                    h.claim_interface(interface_desc.interface_number())
                        .unwrap();
                    // h.set_alternate_setting(interface_desc.interface_number(), interface_desc.setting_number()).unwrap();

                    break 'outer;
                }
            }
        }
    }

    // not entirely sure what these do, but do them for consistency
    make_req(&mut h, b"ST");
    make_req(&mut h, b"RT0");
    make_req(&mut h, b"RT1");
    make_req(&mut h, b"MN");
    make_req(&mut h, b"SAr");
    make_req(&mut h, b"FTr");
    make_req(&mut h, b"FV");
    make_req(&mut h, b"IUr");

    let mut cap_infos = BTreeMap::new();
    let info = get_storage_info(&mut h);
    for title in 1..=info.num_titles {
        let title_info = get_title_info(&mut h, title);
        for local_capture_id in 1..=title_info.num_captures {
            let global_id = get_global_capture_id(&mut h, title, local_capture_id);
            let cap_info = get_capture_info(&mut h, global_id);
            println!(
                "{:2}: {} {} {}",
                global_id, cap_info.title, local_capture_id, cap_info.cct_k
            );
            cap_infos.insert(global_id, (cap_info, local_capture_id));
        }
    }

    println!("select a number to dump");
    let mut line = String::new();
    let (global_id, (ci, local_capture_id)) = loop {
        stdin().read_line(&mut line).unwrap();
        match line.trim().parse() {
            Ok(i) => match cap_infos.get(&i) {
                Some(ci) => break (i, ci),
                None => println!("{i} was not a valid choice"),
            },
            Err(_) => println!("enter a number"),
        }
    };
    println!("enter filename: ");
    line.clear();
    stdin().read_line(&mut line).unwrap();
    write_csv(
        &get_capture_data(&mut h, global_id),
        ci,
        *local_capture_id,
        Path::new(&line.trim()),
    );

    h.unconfigure().unwrap();
}
