use for_streams::comp;

#[test]
fn do_it() {
    let v: Vec<i32> = comp![x + 1 for x in 0..10 if x % 2 == 0].collect();
    assert_eq!(v, vec![1, 3, 5, 7, 9]);
}
