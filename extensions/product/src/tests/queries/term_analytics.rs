use super::Rng;
use crate::queries::*;
use crate::tests::G;
use specforge_extension_sdk::prelude::CommandGraph;

#[test]
fn term_analytics_read_see_also_between_terms_only() {
    // a -> b -> c, a -> a, a -> f (a feature), d -> e, iso alone.
    let g = G::default()
        .n("a", "term")
        .n("b", "term")
        .n("c", "term")
        .n("d", "term")
        .n("e", "term")
        .n("iso", "term")
        .n("f", "feature")
        .edge("a", "b", "see_also")
        .edge("b", "c", "see_also")
        .edge("a", "a", "see_also")
        .edge("a", "f", "see_also")
        .edge("d", "e", "see_also")
        .edge("d", "e", "see_also")
        .build();
    let tg = |id: &str, hops: Option<usize>| term_graph(&g, id, hops).unwrap().related_terms;
    assert_eq!(tg("a", None), ["b"]);
    assert_eq!(tg("a", Some(2)), ["b", "c"]);
    assert_eq!(tg("a", Some(0)), Vec::<String>::new());
    assert_eq!(term_graph(&g, "a", Some(9)).unwrap().max_hops, 5);
    // A see_also is followed from its source.
    assert_eq!(tg("c", Some(5)), Vec::<String>::new());
    assert!(term_graph(&g, "f", None).is_none());
    let tc = term_clusters(&g);
    let clusters: Vec<(usize, Vec<String>)> = tc
        .clusters
        .iter()
        .map(|c| (c.cluster_id, c.term_ids.clone()))
        .collect();
    assert_eq!(
        clusters,
        [
            (1, vec!["a".into(), "b".into(), "c".into()]),
            (2, vec!["d".into(), "e".into()])
        ]
    );
    assert_eq!((tc.isolated_count, tc.total_terms), (1, 6));
    let td = term_density(&g);
    assert_eq!(
        (td.total_terms, td.total_see_also, td.max_connections),
        (6, 3, 2)
    );
    assert_eq!(td.avg_connections, Some(0.5));
    assert_eq!(td.isolated_terms, ["iso"]);
    assert!(td.hub_terms.is_empty());
    let empty = term_density(&CommandGraph::default());
    assert_eq!((empty.total_terms, empty.avg_connections), (0, None));
}

#[test]
fn random_term_graphs_reach_cluster_and_count_as_brute_force_does() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for round in 0..300 {
        let n = rng.below(12) as usize;
        let id = |i: usize| format!("t{i:02}");
        let mut g = G::default().n("x", "feature");
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            g = g.n(&id(i), "term");
        }
        for i in 0..n {
            for j in 0..n {
                if rng.below(6) == 0 {
                    g = g.edge(&id(i), &id(j), "see_also");
                    if i != j && !edges.contains(&(i, j)) {
                        edges.push((i, j));
                    }
                }
            }
            if rng.below(4) == 0 {
                g = g.edge(&id(i), "x", "see_also");
            }
        }
        let g = g.build();
        let linked = |u: usize, v: usize| edges.contains(&(u, v)) || edges.contains(&(v, u));

        // term_graph: the terms within h directed hops, by relaxation.
        for t in 0..n {
            let hops = rng.below(7) as usize;
            let mut within = vec![t];
            for _ in 0..hops.min(5) {
                let next: Vec<usize> = edges
                    .iter()
                    .filter(|(u, _)| within.contains(u))
                    .map(|&(_, v)| v)
                    .collect();
                within.extend(next);
                within.sort_unstable();
                within.dedup();
            }
            let expected: Vec<String> = within.into_iter().filter(|&v| v != t).map(id).collect();
            let got = term_graph(&g, &id(t), Some(hops)).unwrap();
            assert_eq!(
                got.related_terms, expected,
                "round {round}: t{t:02}, {hops} hops"
            );
            assert_eq!(got.max_hops, hops.min(5));
        }

        // term_clusters: the components of the undirected links, by
        // repeated merging; isolated terms in none.
        let mut label: Vec<usize> = (0..n).collect();
        loop {
            let mut changed = false;
            for u in 0..n {
                for v in 0..n {
                    if linked(u, v) && label[u] != label[v] {
                        let low = label[u].min(label[v]);
                        label[u] = low;
                        label[v] = low;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let degree: Vec<usize> = (0..n)
            .map(|u| (0..n).filter(|&v| linked(u, v)).count())
            .collect();
        let mut components: Vec<Vec<String>> = (0..n)
            .filter(|&r| label[r] == r && degree[r] > 0)
            .map(|r| (0..n).filter(|&u| label[u] == r).map(id).collect())
            .collect();
        components
            .sort_by(|a: &Vec<String>, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
        let tc = term_clusters(&g);
        let got: Vec<Vec<String>> = tc.clusters.iter().map(|c| c.term_ids.clone()).collect();
        assert_eq!(got, components, "round {round}");
        let isolated = degree.iter().filter(|&&d| d == 0).count();
        assert_eq!(tc.isolated_count, isolated);
        assert_eq!(
            tc.clusters.iter().map(|c| c.term_count).sum::<usize>() + tc.isolated_count,
            n
        );
        assert!(tc
            .clusters
            .iter()
            .enumerate()
            .all(|(i, c)| c.cluster_id == i + 1));

        // term_density: the counts, average, hubs and isolated terms.
        let td = term_density(&g);
        assert_eq!(
            (td.total_terms, td.total_see_also),
            (n, edges.len()),
            "round {round}"
        );
        let avg = (n > 0).then(|| edges.len() as f64 / n as f64);
        assert_eq!(td.avg_connections, avg);
        assert_eq!(
            td.max_connections,
            degree.iter().copied().max().unwrap_or(0)
        );
        let hubs: Vec<String> = (0..n)
            .filter(|&u| degree[u] >= 3 && degree[u] as f64 > 2.0 * avg.unwrap_or(0.0))
            .map(id)
            .collect();
        assert_eq!(td.hub_terms, hubs, "round {round}");
        let lone: Vec<String> = (0..n).filter(|&u| degree[u] == 0).map(id).collect();
        assert_eq!(td.isolated_terms, lone);
    }
}
