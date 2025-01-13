use {super::*, ord::sat::Sat, ord::subcommand::epochs::Output};

#[test]
fn empty() {
  assert_eq!(
    CommandBuilder::new("epochs").output::<Output>(),
    Output {
      starting_sats: vec![
        Sat(0 * COIN_VALUE),
        Sat(100000000000 * COIN_VALUE),
        Sat(122500000000 * COIN_VALUE),
        Sat(136250000000 * COIN_VALUE),
        Sat(148750000000 * COIN_VALUE),
        Sat(155000000000 * COIN_VALUE),
        Sat(158125000000 * COIN_VALUE),
        Sat(159687500000 * COIN_VALUE),
      ]
    }
  );
}
