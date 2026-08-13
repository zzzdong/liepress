use std::path::Path;
fn main() {
    let img = image::open(Path::new("/tmp/tasklist.png")).unwrap().to_rgb8();
    let (w,h)=img.dimensions();
    for y in [84u32,132,168] {
        let mut s=String::new();
        for x in 0..120u32 {
            let p=img.get_pixel(x,y).0;
            if p[0]<230||p[1]<230||p[2]<230 { s.push('#'); } else { s.push('.'); }
        }
        println!("row {}: |{}|", y, s);
    }
}
