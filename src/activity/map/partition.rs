//! Répartition des lieux d'une carte entre plusieurs serveurs Discord (décision I4,
//! `docs/activity/IMPORT_SPEC.md` § 7.4).
//!
//! Contraintes par serveur : au plus `max_places` lieux (60, Q9), et un budget de salons et de
//! rôles (limites de Discord : 500 salons, 250 rôles). Objectif : **minimiser le nombre de routes
//! entre serveurs**, qui obligent les voyageurs à être membres de deux serveurs.
//!
//! Algorithme (déterministe, sans aléa) :
//! 1. `k` = nombre minimal de serveurs, augmenté tant qu'aucune répartition ne tient ;
//! 2. graines éloignées les unes des autres (point le plus loin des graines déjà choisies) ;
//! 3. croissance de régions : Dijkstra multi-source sur les routes, chaque serveur s'arrêtant à
//!    sa capacité ; les lieux non atteints (îles sans route) vont au serveur le plus proche qui a
//!    de la place ;
//! 4. raffinement : un lieu de bordure change de serveur si cela réduit les routes coupées sans
//!    dépasser une capacité, jusqu'à stabilité.
//!
//! Les lieux déjà rattachés à un serveur (ré-import, lieux existants) y sont **fixés**.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Limites de Discord pour un serveur.
pub const DISCORD_MAX_CHANNELS: u32 = 500;
pub const DISCORD_MAX_ROLES: u32 = 250;
/// Salons par catégorie (les routes sont rangées dans des catégories de 50 au plus).
pub const DISCORD_MAX_CHANNELS_PER_CATEGORY: u32 = 50;
/// Lieux par serveur (Q9).
pub const DEFAULT_MAX_PLACES_PER_SERVER: usize = 60;

/// Ce qu'un serveur contient déjà ou contiendra en plus des lieux et routes répartis ici.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServerLoad {
    pub channels: u32,
    pub roles: u32,
}

/// Lieu à répartir.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub x: f64,
    pub y: f64,
    /// Serveur imposé (index dans `servers`).
    pub pinned: Option<usize>,
    /// Lieu déjà créé : compte dans la limite de lieux, pas dans le budget de salons et de rôles
    /// (déjà inclus dans la charge du serveur).
    pub existing: bool,
}

/// Route entre deux lieux (index dans `nodes`), avec sa longueur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub length: f64,
    /// Route déjà créée : son salon reste sur ce serveur, et compte déjà dans sa charge.
    pub existing_host: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionInput {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Serveurs connus (choisis par l'admin), avec leur charge actuelle (installation,
    /// lieux qui ne font pas partie de la carte…). Des serveurs vides sont ajoutés si besoin.
    pub servers: Vec<ServerLoad>,
    /// Charge d'un serveur ajouté (son installation).
    pub new_server_load: ServerLoad,
    pub max_places: usize,
}

/// Budget d'un serveur après répartition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ServerBudget {
    /// Lieux **à créer** sur ce serveur.
    pub places: u32,
    pub roads_hosted: u32,
    pub road_categories: u32,
    pub channels: u32,
    pub roles: u32,
}

impl ServerBudget {
    pub fn fits(&self, max_places: usize) -> bool {
        self.places as usize <= max_places && self.channels <= DISCORD_MAX_CHANNELS && self.roles <= DISCORD_MAX_ROLES
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Partition {
    /// Serveur de chaque lieu.
    pub server_of: Vec<usize>,
    /// Serveur qui héberge le salon de chaque route.
    pub host_of: Vec<usize>,
    pub budgets: Vec<ServerBudget>,
    /// Routes entre deux serveurs.
    pub cut: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionError {
    /// Un lieu fixé désigne un serveur inconnu.
    BadPin,
    /// Les lieux fixés dépassent à eux seuls la capacité d'un serveur.
    PinnedOverflow(usize),
    /// Aucune répartition trouvée (ne devrait pas arriver : on ajoute des serveurs).
    NoSolution,
}

/// Nombre de catégories de routes pour `roads` salons.
pub fn road_categories(roads: u32) -> u32 {
    roads.div_ceil(DISCORD_MAX_CHANNELS_PER_CATEGORY)
}

/// Budget d'un serveur : un lieu = 1 rôle + 1 catégorie + 1 salon ; une route = 1 salon, dans des
/// catégories de 50.
pub fn budget(load: ServerLoad, places: u32, roads_hosted: u32) -> ServerBudget {
    let cats = road_categories(roads_hosted);
    ServerBudget {
        places,
        roads_hosted,
        road_categories: cats,
        channels: load.channels + 2 * places + roads_hosted + cats,
        roles: load.roles + places,
    }
}

#[derive(Copy, Clone, PartialEq)]
struct Item(f64, usize, usize); // (distance, nœud, serveur)
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal).then_with(|| o.1.cmp(&self.1)).then_with(|| o.2.cmp(&self.2))
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

fn d2(a: &Node, b: &Node) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

/// Répartit les lieux. Voir le module.
pub fn partition(input: &PartitionInput) -> Result<Partition, PartitionError> {
    let n = input.nodes.len();
    let known = input.servers.len();
    for node in &input.nodes {
        if node.pinned.is_some_and(|s| s >= known) {
            return Err(PartitionError::BadPin);
        }
    }
    let mut pinned_count = vec![0usize; known];
    for s in input.nodes.iter().filter_map(|x| x.pinned) {
        pinned_count[s] += 1;
    }
    if let Some(s) = pinned_count.iter().position(|&c| c > input.max_places) {
        return Err(PartitionError::PinnedOverflow(s));
    }
    if n == 0 {
        let budgets = input.servers.iter().map(|&l| budget(l, 0, 0)).collect();
        return Ok(Partition { server_of: vec![], host_of: vec![], budgets, cut: 0 });
    }
    let min_k = n.div_ceil(input.max_places.max(1)).max(known).max(1);
    for k in min_k..=n.max(min_k) + known {
        if let Some(p) = try_k(input, k) {
            return Ok(p);
        }
    }
    Err(PartitionError::NoSolution)
}

fn loads(input: &PartitionInput, k: usize) -> Vec<ServerLoad> {
    (0..k).map(|s| input.servers.get(s).copied().unwrap_or(input.new_server_load)).collect()
}

fn try_k(input: &PartitionInput, k: usize) -> Option<Partition> {
    let n = input.nodes.len();
    let cap = input.max_places;
    let adj = adjacency(n, &input.edges);
    let mut server_of: Vec<Option<usize>> = input.nodes.iter().map(|x| x.pinned).collect();
    let mut size = vec![0usize; k];
    for s in server_of.iter().flatten() {
        size[*s] += 1;
    }

    // Graines : un lieu fixé si le serveur en a, sinon le lieu libre le plus éloigné des graines.
    let mut seeds: Vec<Option<usize>> = (0..k).map(|s| input.nodes.iter().position(|x| x.pinned == Some(s))).collect();
    for s in 0..k {
        if seeds[s].is_some() {
            continue;
        }
        let chosen: Vec<usize> = seeds.iter().flatten().copied().collect();
        let candidate = (0..n)
            .filter(|&i| server_of[i].is_none() && !chosen.contains(&i))
            .max_by(|&i, &j| {
                let di = chosen.iter().map(|&c| d2(&input.nodes[i], &input.nodes[c])).fold(f64::INFINITY, f64::min);
                let dj = chosen.iter().map(|&c| d2(&input.nodes[j], &input.nodes[c])).fold(f64::INFINITY, f64::min);
                // Sans graine : le lieu le plus à l'ouest puis au nord (déterministe).
                let (di, dj) = if chosen.is_empty() { (-input.nodes[i].x - input.nodes[i].y, -input.nodes[j].x - input.nodes[j].y) } else { (di, dj) };
                di.partial_cmp(&dj).unwrap_or(Ordering::Equal).then(j.cmp(&i))
            });
        if let Some(c) = candidate {
            seeds[s] = Some(c);
            server_of[c] = Some(s);
            size[s] += 1;
        }
    }

    // Croissance de régions.
    let mut heap = BinaryHeap::new();
    for (s, seed) in seeds.iter().enumerate() {
        if let Some(seed) = seed {
            heap.push(Item(0.0, *seed, s));
        }
    }
    for i in 0..n {
        if let Some(s) = input.nodes[i].pinned {
            heap.push(Item(0.0, i, s));
        }
    }
    let mut best = vec![f64::INFINITY; n];
    while let Some(Item(d, u, s)) = heap.pop() {
        if server_of[u] != Some(s) {
            continue;
        }
        if d > best[u] {
            continue;
        }
        best[u] = d;
        for &(v, len) in &adj[u] {
            if server_of[v].is_none() && size[s] < cap {
                server_of[v] = Some(s);
                size[s] += 1;
                heap.push(Item(d + len, v, s));
            }
        }
    }
    // Lieux non atteints : serveur le plus proche (à vol d'oiseau) qui a de la place.
    for i in 0..n {
        if server_of[i].is_some() {
            continue;
        }
        let target = (0..n)
            .filter_map(|j| server_of[j].map(|s| (s, d2(&input.nodes[i], &input.nodes[j]))))
            .filter(|&(s, _)| size[s] < cap)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal).then(a.0.cmp(&b.0)))
            .map(|(s, _)| s)
            .or_else(|| (0..k).find(|&s| size[s] < cap))?;
        server_of[i] = Some(target);
        size[target] += 1;
    }
    let mut server_of: Vec<usize> = server_of.into_iter().map(|s| s.unwrap()).collect();

    refine(input, &adj, &mut server_of, &mut size, cap);

    // Hébergement des routes et budgets.
    let ls = loads(input, k);
    let mut new_places = vec![0u32; k];
    for (i, node) in input.nodes.iter().enumerate() {
        if !node.existing {
            new_places[server_of[i]] += 1;
        }
    }
    let mut hosted = vec![0u32; k];
    let mut host_of = Vec::with_capacity(input.edges.len());
    for e in &input.edges {
        if let Some(h) = e.existing_host.filter(|&h| h < k) {
            host_of.push(h);
            continue;
        }
        let (sa, sb) = (server_of[e.a], server_of[e.b]);
        let host = if sa == sb {
            sa
        } else {
            // Le serveur qui a le plus de salons libres.
            let free = |s: usize| DISCORD_MAX_CHANNELS as i64 - budget(ls[s], new_places[s], hosted[s]).channels as i64;
            if free(sb) > free(sa) { sb } else { sa }
        };
        hosted[host] += 1;
        host_of.push(host);
    }
    let budgets: Vec<ServerBudget> = (0..k).map(|s| budget(ls[s], new_places[s], hosted[s])).collect();
    if !budgets.iter().all(|b| b.fits(cap)) || size.iter().any(|&c| c > cap) {
        return None;
    }
    let cut = input.edges.iter().filter(|e| server_of[e.a] != server_of[e.b]).count();
    Some(Partition { server_of, host_of, budgets, cut })
}

fn adjacency(n: usize, edges: &[Edge]) -> Vec<Vec<(usize, f64)>> {
    let mut adj = vec![vec![]; n];
    for e in edges {
        if e.a != e.b && e.a < n && e.b < n {
            adj[e.a].push((e.b, e.length.max(0.0)));
            adj[e.b].push((e.a, e.length.max(0.0)));
        }
    }
    for a in &mut adj {
        a.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(Ordering::Equal).then(x.0.cmp(&y.0)));
    }
    adj
}

/// Déplace les lieux de bordure tant que cela réduit les routes coupées.
fn refine(input: &PartitionInput, adj: &[Vec<(usize, f64)>], server_of: &mut [usize], size: &mut [usize], cap: usize) {
    let k = size.len();
    for _pass in 0..50 {
        let mut moved = false;
        for u in 0..server_of.len() {
            if input.nodes[u].pinned.is_some() {
                continue;
            }
            let from = server_of[u];
            let mut links = vec![0i64; k];
            for &(v, _) in &adj[u] {
                links[server_of[v]] += 1;
            }
            let best = (0..k)
                .filter(|&s| s != from && size[s] < cap && size[from] > 1)
                .map(|s| (s, links[s] - links[from]))
                .filter(|&(_, gain)| gain > 0)
                .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)));
            if let Some((to, _)) = best {
                server_of[u] = to;
                size[from] -= 1;
                size[to] += 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize) -> (Vec<Node>, Vec<Edge>) {
        let nodes = (0..n).map(|i| Node { x: i as f64, y: 0.0, pinned: None, existing: false }).collect();
        let edges = (1..n).map(|i| Edge { a: i - 1, b: i, length: 1.0, existing_host: None }).collect();
        (nodes, edges)
    }

    fn input(nodes: Vec<Node>, edges: Vec<Edge>, max_places: usize) -> PartitionInput {
        PartitionInput { nodes, edges, servers: vec![], new_server_load: ServerLoad { channels: 11, roles: 4 }, max_places }
    }

    #[test]
    fn a_small_map_fits_on_one_server() {
        let (n, e) = line(10);
        let p = partition(&input(n, e, 60)).unwrap();
        assert_eq!(p.budgets.len(), 1);
        assert_eq!(p.cut, 0);
        assert_eq!(p.budgets[0], ServerBudget { places: 10, roads_hosted: 9, road_categories: 1, channels: 11 + 20 + 9 + 1, roles: 14 });
    }

    #[test]
    fn a_line_is_cut_once_per_extra_server() {
        let (n, e) = line(120);
        let p = partition(&input(n, e, 60)).unwrap();
        assert_eq!(p.budgets.len(), 2);
        assert_eq!(p.cut, 1, "one road between the two halves");
        assert!(p.budgets.iter().all(|b| b.places == 60));
    }

    #[test]
    fn two_clusters_joined_by_one_road_are_split_on_that_road() {
        // Deux grappes de 50 lieux très reliées, réunies par une seule route.
        let mut nodes = vec![];
        let mut edges = vec![];
        for c in 0..2 {
            for i in 0..50 {
                nodes.push(Node { x: c as f64 * 1000.0 + (i % 7) as f64, y: (i / 7) as f64, pinned: None, existing: false });
            }
            let base = c * 50;
            for i in 0..50 {
                for j in [i + 1, i + 7] {
                    if j < 50 {
                        edges.push(Edge { a: base + i, b: base + j, length: 1.0, existing_host: None });
                    }
                }
            }
        }
        edges.push(Edge { a: 49, b: 50, length: 500.0, existing_host: None });
        let p = partition(&input(nodes, edges, 60)).unwrap();
        assert_eq!(p.budgets.len(), 2);
        assert_eq!(p.cut, 1);
        assert!(p.server_of[..50].iter().all(|&s| s == p.server_of[0]));
        assert!(p.server_of[50..].iter().all(|&s| s == p.server_of[50]));
    }

    #[test]
    fn pinned_places_stay_on_their_server() {
        let (mut n, e) = line(30);
        n[29].pinned = Some(0);
        let mut i = input(n, e, 60);
        i.servers = vec![ServerLoad { channels: 11, roles: 4 }];
        let p = partition(&i).unwrap();
        assert_eq!(p.server_of[29], 0);
    }

    #[test]
    fn existing_places_and_roads_are_not_counted_twice() {
        let (mut n, mut e) = line(3);
        for x in &mut n[..2] {
            x.pinned = Some(0);
            x.existing = true;
        }
        e[0].existing_host = Some(0);
        let mut i = input(n, e, 60);
        i.servers = vec![ServerLoad { channels: 20, roles: 6 }];
        let p = partition(&i).unwrap();
        assert_eq!(p.budgets.len(), 1);
        // Un seul lieu et une seule route à créer.
        assert_eq!(p.budgets[0], ServerBudget { places: 1, roads_hosted: 1, road_categories: 1, channels: 20 + 2 + 1 + 1, roles: 7 });
    }

    #[test]
    fn too_many_pinned_places_are_refused() {
        let (mut n, e) = line(5);
        for x in &mut n {
            x.pinned = Some(0);
        }
        let mut i = input(n, e, 3);
        i.servers = vec![ServerLoad::default()];
        assert_eq!(partition(&i), Err(PartitionError::PinnedOverflow(0)));
        let (mut n, e) = line(2);
        n[0].pinned = Some(4);
        assert_eq!(partition(&input(n, e, 3)), Err(PartitionError::BadPin));
    }

    #[test]
    fn a_full_server_gets_a_new_one() {
        // Serveur connu déjà presque plein en salons : la carte part sur un nouveau serveur.
        let (n, e) = line(10);
        let mut i = input(n, e, 60);
        i.servers = vec![ServerLoad { channels: 495, roles: 10 }];
        let p = partition(&i).unwrap();
        assert!(p.budgets.len() >= 2);
        assert!(p.budgets.iter().all(|b| b.fits(60)));
    }

    #[test]
    fn islands_without_roads_are_placed() {
        let nodes = (0..5).map(|i| Node { x: i as f64 * 100.0, y: 0.0, pinned: None, existing: false }).collect();
        let p = partition(&input(nodes, vec![], 2)).unwrap();
        assert_eq!(p.budgets.len(), 3);
        assert_eq!(p.budgets.iter().map(|b| b.places).sum::<u32>(), 5);
    }

    #[test]
    fn the_result_is_deterministic() {
        let (n, e) = line(200);
        let i = input(n, e, 60);
        assert_eq!(partition(&i), partition(&i));
    }

    #[test]
    fn road_categories_hold_50_channels() {
        assert_eq!(road_categories(0), 0);
        assert_eq!(road_categories(50), 1);
        assert_eq!(road_categories(51), 2);
    }
}
