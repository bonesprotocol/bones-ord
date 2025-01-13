use super::*;
use crate::sat::Sat;
use crate::sat_point::SatPoint;

#[derive(Boilerplate)]
pub(crate) struct RareTxt(pub(crate) Vec<(Sat, SatPoint)>);
