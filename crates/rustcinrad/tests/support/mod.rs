#![allow(dead_code)]
use rustcinrad::*;

pub fn u16_at(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}

pub fn i32_at(b: &mut [u8], o: usize, v: i32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

pub fn f32_at(b: &mut [u8], o: usize, v: f32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

pub fn site() -> RadarSite {
    RadarSite {
        code: "TEST".into(),
        name: "Test radar".into(),
        position: GeoPoint::new(120.0, 30.0).unwrap(),
        altitude_m: 100.0,
    }
}

pub fn options() -> ReadOptions {
    ReadOptions { site: Some(site()), format: Some(RadarFormat::Sab), ..Default::default() }
}

pub fn sab(azimuth: f64, number: u16, end: bool) -> Vec<u8> {
    let mut b = vec![0; 2432];
    b[28..32].copy_from_slice(&1000_u32.to_le_bytes());
    for (o, v) in [
        (32, 17750),
        (36, (azimuth / (180.0 / 4096.0 * 0.125)).round() as u16),
        (38, number),
        (40, if end { 4 } else { 1 }),
        (42, 91),
        (44, 1),
        (50, 1000),
        (52, 250),
        (54, 4),
        (56, 4),
        (64, 100),
        (66, 332),
        (68, 432),
        (70, 2),
        (72, 21),
        (88, 2000),
    ] {
        u16_at(&mut b, o, v);
    }
    b[128..132].copy_from_slice(&[0, 1, 66, 106]);
    // Deliberate non-contiguous moment blocks: catches sequential payload assumptions.
    b[360..364].copy_from_slice(&[0, 1, 129, 169]);
    b[460..464].copy_from_slice(&[0, 1, 129, 169]);
    b
}

pub fn volume() -> RadarDataset {
    let b: Vec<_> = (0..360).flat_map(|a| sab(a as f64, a + 1, a == 359)).collect();
    read_bytes(&b, "test", &options()).unwrap()
}

pub fn fmt(pa: bool, bin_length: i16, code: i32) -> Vec<u8> {
    let radial_size = if pa { 128 } else { 64 };
    let mut b = vec![0; 672 + radial_size + 32 + 4 * bin_length as usize];
    b[..4].copy_from_slice(b"RSTM");
    i32_at(&mut b, 8, if pa { 16 } else { 1 });
    b[32..36].copy_from_slice(b"TEST");
    f32_at(&mut b, 72, 30.0);
    f32_at(&mut b, 76, 120.0);
    i32_at(&mut b, 80, 100);
    i32_at(&mut b, if pa { 332 } else { 336 }, 1);
    if pa {
        f32_at(&mut b, 416 + 72, 1000.0);
        f32_at(&mut b, 416 + 76, 250.0);
        f32_at(&mut b, 416 + 108, 20.0);
        f32_at(&mut b, 416 + 64, 1.0);
    } else {
        i32_at(&mut b, 416 + 44, 1000);
        i32_at(&mut b, 416 + 48, 250);
        f32_at(&mut b, 416 + 80, 20.0);
        f32_at(&mut b, 416 + 36, 1.0);
    }
    i32_at(&mut b, 672, 4);
    i32_at(&mut b, 672 + 16, 1);
    f32_at(&mut b, 672 + 20, 0.0);
    f32_at(&mut b, 672 + 24, 0.5);
    i32_at(&mut b, 672 + 28, 1_700_000_000);
    i32_at(&mut b, 672 + if pa { 40 } else { 36 }, 32 + 4 * bin_length as i32);
    i32_at(&mut b, 672 + if pa { 44 } else { 40 }, 1);
    let m = 672 + radial_size;
    i32_at(&mut b, m, code);
    i32_at(&mut b, m + 4, 2);
    i32_at(&mut b, m + 8, 66);
    b[m + 12..m + 14].copy_from_slice(&bin_length.to_le_bytes());
    i32_at(&mut b, m + 16, 4 * bin_length as i32);
    for (i, v) in [0_u16, 1, 66, 106].iter().enumerate() {
        if bin_length == 1 {
            b[m + 32 + i] = *v as u8;
        } else {
            u16_at(&mut b, m + 32 + i * 2, *v);
        }
    }
    b
}

pub fn cc() -> Vec<u8> {
    let mut b = vec![0; 1024 + 3 * 3000];
    b[66..77].copy_from_slice(b"Native site");
    b[106..111].copy_from_slice(b"Z1234");
    b[116..125].copy_from_slice(b"CINRAD/CC");
    i32_at(&mut b, 168, 432_000_000);
    i32_at(&mut b, 172, 108_000_000);
    i32_at(&mut b, 176, 100_000);
    b[184] = 20;
    b[185] = 26;
    b[186] = 11;
    b[187] = 19;
    b[188] = 8;
    b[199] = 101;
    for (o, v) in [(218, 2000), (222, 150), (224, 2), (226, 3), (236, 50)] {
        u16_at(&mut b, o, v);
    }
    for ray in 0..3 {
        for field in 0..3 {
            u16_at(&mut b, 1024 + ray * 3000 + field * 1000, (-32768_i16) as u16);
            u16_at(&mut b, 1026 + ray * 3000 + field * 1000, 300);
        }
    }
    b
}

pub fn cd() -> Vec<u8> {
    let mut b = vec![0; 1024 + 3 * 4000];
    b[50..61].copy_from_slice(b"Native site");
    b[90..95].copy_from_slice(b"Z1234");
    b[100..109].copy_from_slice(b"CINRAD/CD");
    i32_at(&mut b, 152, 12000);
    i32_at(&mut b, 156, 3000);
    i32_at(&mut b, 160, 100000);
    b[201] = 101;
    u16_at(&mut b, 202, 2026);
    b[204] = 11;
    b[205] = 19;
    b[206] = 8;
    for (o, v) in [(226, 2000), (230, 3000), (232, 2), (234, 3), (236, 50)] {
        u16_at(&mut b, o, v);
    }
    for ray in 0..3 {
        b[1024 + ray * 4000 + 8..1024 + ray * 4000 + 16].copy_from_slice(&[0, 0, 0, 0, 124, 128, 124, 128]);
    }
    b
}

pub fn cc2() -> Vec<u8> {
    let mut b = vec![0; 2060 + 2 * 49163];
    b[62..73].copy_from_slice(b"Native site");
    b[102..107].copy_from_slice(b"Z1234");
    b[112..122].copy_from_slice(b"CINRAD/CC2");
    i32_at(&mut b, 164, 120000);
    i32_at(&mut b, 168, 30000);
    i32_at(&mut b, 172, 100000);
    b[216] = 101;
    u16_at(&mut b, 217, 2026);
    b[219] = 11;
    b[220] = 19;
    b[221] = 8;
    let p = 238;
    for (o, v) in [
        (p + 10, 2000),
        (p + 14, 3000),
        (p + 16, 3000),
        (p + 20, 2),
        (p + 22, 2),
        (p + 24, 2),
        (p + 26, 2),
        (p + 28, 50),
    ] {
        u16_at(&mut b, o, v);
    }
    for ray in 0..2 {
        let r = 2060 + ray * 49163;
        u16_at(&mut b, r, 50);
        u16_at(&mut b, r + 2, ray as u16 * 100);
        b[r + 12] = 124;
        for (o, v) in [(16397, 100), (24589, 26000), (32781, 990), (40973, 10)] {
            u16_at(&mut b, r + o, v);
        }
    }
    b
}

pub fn legacy_pa() -> Vec<u8> {
    let mut b = vec![0; 4 * 16104];
    for ray in 0..4 {
        let r = ray * 16104;
        u16_at(&mut b, r + 2, 21845);
        u16_at(&mut b, r + 4, 5461);
        u16_at(&mut b, r + 8, 6553);
        i32_at(&mut b, r + 32, 1700000000);
        u16_at(&mut b, r + 42, (ray / 2 * 65535 / 360) as u16);
        u16_at(&mut b, r + 44, 2);
        u16_at(&mut b, r + 46, (91 + ray % 2 * 91) as u16);
        u16_at(&mut b, r + 48, 2);
        u16_at(&mut b, r + 54, 65535);
        u16_at(&mut b, r + 56, 2);
        for field in 0..8 {
            b[r + 100 + field * 2000] = 255;
            b[r + 101 + field * 2000] = 100;
        }
    }
    b
}

pub fn swan() -> Vec<u8> {
    let mut b = vec![0; 1024 + 8];
    b[..4].copy_from_slice(b"SWAN");
    b[12..14].copy_from_slice(b"CR");
    for (o, v) in [(66, 2026), (68, 11), (70, 19), (72, 8), (78, 2), (80, 2), (82, 2)] {
        u16_at(&mut b, o, v);
    }
    for (o, v) in [(88, 120.0), (92, 30.0), (96, 120.5), (100, 30.5), (112, 1.0), (116, 2.0)] {
        f32_at(&mut b, o, v);
    }
    b[1024..].copy_from_slice(&[66, 68, 70, 0, 80, 82, 84, 86]);
    b
}

pub fn moc(single: bool) -> Vec<u8> {
    let mut b = if single { vec![0; 758 + 2 * (132 + 8)] } else { vec![0; 256 + 8] };
    b[..4].copy_from_slice(b"MOC3");
    if single {
        b[12..17].copy_from_slice(b"Z1234");
        b[20..31].copy_from_slice(b"Native site");
        i32_at(&mut b, 72, 300000);
        i32_at(&mut b, 76, 1200000);
        i32_at(&mut b, 80, 100000);
        i32_at(&mut b, 84, 28800);
        for (o, v) in [(88, 2026), (90, 11), (92, 19), (94, 8), (112, 2)] {
            u16_at(&mut b, o, v);
        }
        for layer in 0..2 {
            let p = 758 + layer * 140;
            b[p + 4..p + 8].copy_from_slice(b"QREF");
            for (o, v) in [(64, 30000), (68, 120000), (72, 31000), (76, 121000), (88, 2), (92, 2)] {
                i32_at(&mut b, p + o, v);
            }
            u16_at(&mut b, p + 110, (layer as u16 + 1) * 1000);
            u16_at(&mut b, p + 120, 10);
            for (i, v) in [100_i16 + layer as i16 * 100, 200, -1, 400].iter().enumerate() {
                u16_at(&mut b, p + 132 + i * 2, *v as u16);
            }
        }
    } else {
        b[16..20].copy_from_slice(b"CREF");
        i32_at(&mut b, 96, 28800);
        for (o, v) in [(100, 2026), (102, 11), (104, 19), (106, 8), (164, 1000), (176, 10)] {
            u16_at(&mut b, o, v);
        }
        for (o, v) in [(124, 30000), (128, 120000), (132, 31000), (136, 121000), (148, 2), (152, 2)] {
            i32_at(&mut b, o, v);
        }
        for (i, v) in [100_i16, 200, -1, 400].iter().enumerate() {
            u16_at(&mut b, 256 + i * 2, *v as u16);
        }
    }
    b
}

pub fn standard_pup(word: i16) -> Vec<u8> {
    let mut b = vec![0; 416 + 192 + 64 + 2 * (32 + 4 * word as usize)];
    b[..4].copy_from_slice(b"RSTM");
    i32_at(&mut b, 8, 2);
    b[32..37].copy_from_slice(b"Z9411");
    b[40..46].copy_from_slice(b"Native");
    f32_at(&mut b, 72, 30.0);
    f32_at(&mut b, 76, 120.0);
    i32_at(&mut b, 80, 100);
    i32_at(&mut b, 416, 1);
    i32_at(&mut b, 456, 1700000000);
    f32_at(&mut b, 544, 0.5);
    i32_at(&mut b, 608, 2);
    i32_at(&mut b, 612, 2);
    i32_at(&mut b, 616, 66);
    u16_at(&mut b, 620, word as u16);
    i32_at(&mut b, 624, 1000);
    i32_at(&mut b, 632, 4000);
    i32_at(&mut b, 636, 2);
    for ray in 0..2 {
        let p = 672 + ray * (32 + 4 * word as usize);
        f32_at(&mut b, p, ray as f32 * 90.0);
        f32_at(&mut b, p + 4, 1.0);
        i32_at(&mut b, p + 8, 4);
        for (i, v) in [0_u16, 1, 66, 106].into_iter().enumerate() {
            if word == 1 {
                b[p + 32 + i] = v as u8;
            } else {
                u16_at(&mut b, p + 32 + i * 2, v);
            }
        }
    }
    b
}
