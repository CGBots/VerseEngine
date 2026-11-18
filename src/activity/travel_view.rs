//! Vue « temps réel » d'un déplacement pour le client : le compte à rebours est
//! calculé côté client à partir de `eta_timestamp` et resynchronisé à chaque évènement.

use serde::Serialize;

use crate::database::travel::{SpaceType, TravelGroup};
use crate::travel::logic::calculate_current_distance_at;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TravelView {
    pub in_move: bool,
    /// Fin de l'étape courante (changement de vitesse ou arrivée).
    pub step_end_timestamp: Option<u64>,
    /// Arrivée estimée à vitesse constante.
    pub eta_timestamp: Option<u64>,
    pub remaining_km: Option<f64>,
    pub traveled_km: f64,
    pub speed_kmh: f64,
    pub final_step: bool,
}

/// `road_distance_km` : longueur de la route empruntée (si sur une route).
pub fn travel_view(group: &TravelGroup, road_distance_km: Option<f64>, now: u64) -> TravelView {
    let traveled = if group.actual_space_type == SpaceType::Road { calculate_current_distance_at(group, now) } else { 0.0 };
    let remaining = road_distance_km.filter(|_| group.actual_space_type == SpaceType::Road).map(|d| (d - traveled).max(0.0));
    let eta = if !group.is_in_move {
        None
    } else if group.is_end {
        group.step_end_timestamp
    } else {
        match remaining {
            Some(r) if group.modified_speed > 0.0 => Some(now + ((r / group.modified_speed) * 3600.0).ceil() as u64),
            _ => None,
        }
    };
    TravelView {
        in_move: group.is_in_move,
        step_end_timestamp: group.step_end_timestamp.filter(|_| group.is_in_move),
        eta_timestamp: eta,
        remaining_km: remaining,
        traveled_km: traveled,
        speed_kmh: group.modified_speed,
        final_step: group.is_in_move && group.is_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moving(start: u64, end: u64, speed: f64, traveled: f64, is_end: bool) -> TravelGroup {
        TravelGroup {
            actual_space_type: SpaceType::Road,
            is_in_move: true,
            is_end,
            step_start_timestamp: Some(start),
            step_end_timestamp: Some(end),
            modified_speed: speed,
            distance_traveled: traveled,
            ..Default::default()
        }
    }

    #[test]
    fn final_step_uses_step_end() {
        let g = moving(1000, 4600, 10.0, 0.0, true);
        let v = travel_view(&g, Some(10.0), 2800);
        assert_eq!(v.eta_timestamp, Some(4600));
        assert!(v.final_step);
        assert!((v.traveled_km - 5.0).abs() < 1e-9);
        assert!((v.remaining_km.unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn intermediate_step_extrapolates_at_current_speed() {
        // 100 km, 10 km/h, étape qui s'arrête à t=1000+1800 (fin d'un bonus).
        let g = moving(1000, 2800, 10.0, 20.0, false);
        let v = travel_view(&g, Some(100.0), 1000);
        assert_eq!(v.eta_timestamp, Some(1000 + 8 * 3600));
        assert_eq!(v.step_end_timestamp, Some(2800));
        assert!(!v.final_step);
    }

    #[test]
    fn stopped_or_in_place_has_no_eta() {
        let mut g = moving(1000, 2000, 10.0, 3.0, false);
        g.is_in_move = false;
        let v = travel_view(&g, Some(10.0), 1500);
        assert_eq!(v.eta_timestamp, None);
        assert_eq!(v.step_end_timestamp, None);
        let place = TravelGroup::default();
        let v = travel_view(&place, None, 1500);
        assert!(!v.in_move && v.remaining_km.is_none());
    }

    #[test]
    fn zero_speed_has_no_eta() {
        let g = moving(1000, 1000, 0.0, 0.0, false);
        assert_eq!(travel_view(&g, Some(10.0), 1000).eta_timestamp, None);
    }
}
