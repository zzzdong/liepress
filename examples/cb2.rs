use std::path::Path;
fn main() {
    let img = image::open(Path::new("/tmp/tasklist.png"))
        .unwrap()
        .to_rgb8();
    let (w, h) = img.dimensions();
    println!("=== marker column x[50..120] per row (text rows only) ===");
    for y in [84u32, 132, 168] {
        let mut s = String::new();
        for x in 50..120u32 {
            let p = img.get_pixel(x, y).0;
            if p[0] < 200 || p[1] < 200 || p[2] < 200 {
                s.push('#');
            } else {
                s.push('.');
            }
        }
        println!("row {}: |{}|", y, s);
    }
}
