//! Geradores de mídia sintética (figurinhas WebP e áudios Ogg Opus), usados nos testes
//! e pelo gerador de conversas `synth`. Os áudios têm só os cabeçalhos e a duração: não tocam.

use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::webp::WebPEncoder};

const PRE_SKIP: u16 = 312;

fn ogg_crc(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn ogg_page(header_type: u8, granule: u64, seq: u32, packet: &[u8]) -> Vec<u8> {
    assert!(packet.len() < 255 * 255);
    let mut segs = vec![255u8; packet.len() / 255];
    segs.push((packet.len() % 255) as u8);
    let mut page = Vec::with_capacity(27 + segs.len() + packet.len());
    page.extend_from_slice(b"OggS");
    page.push(0);
    page.push(header_type);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&0x5752_4543u32.to_le_bytes()); // serial
    page.extend_from_slice(&seq.to_le_bytes());
    page.extend_from_slice(&[0; 4]); // CRC, preenchido abaixo
    page.push(segs.len() as u8);
    page.extend_from_slice(&segs);
    page.extend_from_slice(packet);
    let crc = ogg_crc(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

/// Arquivo Ogg Opus mínimo com a duração informada.
pub fn opus_file(duration_ms: u64) -> Vec<u8> {
    let mut head = b"OpusHead".to_vec();
    head.push(1); // versão
    head.push(1); // canais
    head.extend_from_slice(&PRE_SKIP.to_le_bytes());
    head.extend_from_slice(&48_000u32.to_le_bytes());
    head.extend_from_slice(&[0, 0, 0]); // ganho, mapeamento
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&5u32.to_le_bytes());
    tags.extend_from_slice(b"synth");
    tags.extend_from_slice(&0u32.to_le_bytes());
    let granule = PRE_SKIP as u64 + duration_ms * 48;
    let mut out = ogg_page(0x02, 0, 0, &head);
    out.extend(ogg_page(0x00, 0, 1, &tags));
    out.extend(ogg_page(0x00, u64::MAX, 2, &[0xFC; 40])); // página sem pacote terminando
    out.extend(ogg_page(0x04, granule, 3, &[0xFC; 60]));
    out
}

/// Figurinha WebP 256×256 com fundo transparente. Designs diferentes geram imagens
/// visualmente distintas; `variant` altera alguns pixels (outro SHA-256, mesma imagem visível),
/// simulando a mesma figurinha reencodada.
pub fn sticker_webp(design: u32, variant: u32) -> Vec<u8> {
    let mut rng = design.wrapping_mul(2_654_435_761).wrapping_add(12_345);
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng
    };
    let blobs: Vec<(f32, f32, f32, [u8; 3])> = (0..5)
        .map(|_| {
            let v = next();
            (
                (v % 200 + 28) as f32,
                ((v >> 8) % 200 + 28) as f32,
                ((v >> 16) % 50 + 20) as f32,
                [(v >> 3) as u8, (v >> 11) as u8, (v >> 19) as u8],
            )
        })
        .collect();
    let mut img = RgbaImage::from_pixel(256, 256, Rgba([0, 0, 0, 0]));
    for (x, y, p) in img.enumerate_pixels_mut() {
        for &(cx, cy, r, c) in &blobs {
            if (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2) < r * r {
                *p = Rgba([c[0], c[1], c[2], 255]);
            }
        }
    }
    for i in 0..variant {
        let p = img.get_pixel_mut(i % 256, 255 - i / 256);
        p.0 = [p.0[0] ^ 1, p.0[1], p.0[2], p.0[3].max(1)];
    }
    let mut out = Vec::new();
    WebPEncoder::new_lossless(&mut out)
        .write_image(img.as_raw(), 256, 256, ExtendedColorType::Rgba8)
        .expect("encode webp");
    out
}
