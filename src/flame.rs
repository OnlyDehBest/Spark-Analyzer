use rustc_hash::FxHashMap as HashMap;
use std::fmt::Write;

#[derive(Clone, Debug, Default)]
pub struct FlameTree {
    names: Vec<String>,
    name_ids: HashMap<String, u32>,
    name: Vec<u32>,
    parent: Vec<u32>,
    own: Vec<u64>,
    children: HashMap<(u32, u32), u32>,
}

impl FlameTree {
    pub fn new(root: &str) -> FlameTree {
        let mut t = FlameTree::default();
        let id = t.intern(root);
        t.name.push(id);
        t.parent.push(0);
        t.own.push(0);
        t
    }

    pub fn intern(&mut self, label: &str) -> u32 {
        if let Some(id) = self.name_ids.get(label) {
            return *id;
        }
        let id = self.names.len() as u32;
        self.names.push(label.to_string());
        self.name_ids.insert(label.to_string(), id);
        id
    }

    pub fn child(&mut self, parent: u32, name: u32) -> u32 {
        if let Some(c) = self.children.get(&(parent, name)) {
            return *c;
        }
        let id = self.name.len() as u32;
        self.name.push(name);
        self.parent.push(parent);
        self.own.push(0);
        self.children.insert((parent, name), id);
        id
    }

    pub fn add(&mut self, node: u32, value: u64) {
        self.own[node as usize] += value;
    }

    pub fn is_empty(&self) -> bool {
        self.own.iter().all(|v| *v == 0)
    }

    fn inclusive(&self) -> Vec<u64> {
        let mut total = self.own.clone();
        for i in (1..total.len()).rev() {
            let p = self.parent[i] as usize;
            total[p] += total[i];
        }
        total
    }

    fn kids(&self) -> Vec<Vec<u32>> {
        let mut kids: Vec<Vec<u32>> = vec![Vec::new(); self.name.len()];
        for i in 1..self.name.len() {
            kids[self.parent[i] as usize].push(i as u32);
        }
        kids
    }

    pub fn to_json(&self) -> String {
        let total = self.inclusive();
        let kids = self.kids();
        let min = (total[0] / 4000).max(1);
        let mut used: HashMap<u32, u32> = HashMap::default();
        let mut names: Vec<&str> = Vec::new();
        let mut tree = String::with_capacity(self.name.len() * 8);
        let mut stack: Vec<(u32, bool)> = vec![(0, false)];
        while let Some((node, closing)) = stack.pop() {
            if closing {
                tree.push(']');
                continue;
            }
            let n = node as usize;
            let next = used.len() as u32;
            let name_id = *used.entry(self.name[n]).or_insert_with(|| {
                names.push(&self.names[self.name[n] as usize]);
                next
            });
            if !tree.is_empty() && !tree.ends_with('[') {
                tree.push(',');
            }
            let _ = write!(tree, "[{name_id},{}", total[n]);
            stack.push((node, true));
            let mut ks: Vec<u32> = kids[n]
                .iter()
                .copied()
                .filter(|k| total[*k as usize] >= min)
                .collect();
            ks.sort_by(|a, b| total[*b as usize].cmp(&total[*a as usize]));
            for k in ks.into_iter().rev() {
                stack.push((k, false));
            }
        }
        let names = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
        format!("{{\"names\":{names},\"tree\":{tree}}}")
    }

    pub fn folded_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut path: Vec<&str> = Vec::new();
        for (i, v) in self.own.iter().enumerate().skip(1) {
            if *v == 0 {
                continue;
            }
            path.clear();
            let mut cur = i;
            while cur != 0 {
                path.push(&self.names[self.name[cur] as usize]);
                cur = self.parent[cur] as usize;
            }
            path.reverse();
            out.push(format!("{} {v}", path.join(";")));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::FlameTree;

    #[test]
    fn builds_inclusive_json_and_folded_lines() {
        let mut t = FlameTree::new("main thread");
        let a = t.intern("A.run");
        let b = t.intern("B.tick");
        let c = t.intern("C.save");
        let na = t.child(0, a);
        let nb = t.child(na, b);
        let nc = t.child(na, c);
        t.add(nb, 300);
        t.add(nc, 100);
        assert_eq!(t.child(na, b), nb);
        assert_eq!(
            t.to_json(),
            r#"{"names":["main thread","A.run","B.tick","C.save"],"tree":[0,400,[1,400,[2,300],[3,100]]]}"#
        );
        let mut lines = t.folded_lines();
        lines.sort();
        assert_eq!(lines, vec!["A.run;B.tick 300", "A.run;C.save 100"]);
    }
}
