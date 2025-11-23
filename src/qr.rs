use qrcode::QrCode;
use qrcode::render::unicode;

pub fn generate_qr(url: &str) -> String {
    let code = QrCode::new(url.as_bytes()).unwrap();

    let image = code.render::<unicode::Dense1x2>()
        // colors are inverted for better visability in terminal
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();

    image
}

pub fn print_qr(url: &str) {
    println!("\n{}\n", generate_qr(url));
    println!("{}", url)
}
