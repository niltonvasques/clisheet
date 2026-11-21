fn main() {
    let mut v = ironcalc::base::get_supported_locales();
    v.sort();
    println!("{:?}", v);
}
