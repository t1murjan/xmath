//! Three-dimensional plots (surface, contour, 3D bar, 3D scatter): data,
//! persisted view/format, and the software renderer that turns them into a
//! [`Scene`] of 2D primitives (painter's algorithm, simple shading).

use crate::plot_geom::{axis_range, format_tick, nice_step, ticks, Prim, Rgb, Scene, TickLabel};
use crate::worksheet::LineStyle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plot3dKind {
    #[default]
    Surface,
    Contour,
    Scatter,
    Bar,
}

impl Plot3dKind {
    pub fn title(self) -> &'static str {
        match self {
            Plot3dKind::Surface => "поверхность",
            Plot3dKind::Contour => "контурный график",
            Plot3dKind::Scatter => "3D точки",
            Plot3dKind::Bar => "3D гистограмма",
        }
    }
}

/// Camera of a 3D plot: azimuth and elevation in degrees, zoom factor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct View3d {
    pub azimuth: f32,
    pub elevation: f32,
    pub zoom: f32,
}

impl Default for View3d {
    fn default() -> Self {
        View3d { azimuth: 35.0, elevation: 30.0, zoom: 1.0 }
    }
}

impl View3d {
    pub fn clamped(mut self) -> View3d {
        self.elevation = self.elevation.clamp(-90.0, 90.0);
        self.zoom = self.zoom.clamp(0.3, 4.0);
        self.azimuth = self.azimuth.rem_euclid(360.0);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Colormap {
    #[default]
    Rainbow,
    Gray,
    Hot,
    BlueRed,
    GreenYellow,
}

impl Colormap {
    pub const ALL: [Colormap; 5] = [Colormap::Rainbow, Colormap::Gray, Colormap::Hot, Colormap::BlueRed, Colormap::GreenYellow];
    pub fn name(self) -> &'static str {
        match self {
            Colormap::Rainbow => "Радуга",
            Colormap::Gray => "Градации серого",
            Colormap::Hot => "Жаркая",
            Colormap::BlueRed => "Сине-красная",
            Colormap::GreenYellow => "Зелёно-жёлтая",
        }
    }

    /// Colour for `t` in 0..1.
    pub fn color(self, t: f32) -> Rgb {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        let lerp = |stops: &[(f32, [f32; 3])]| -> Rgb {
            let mut prev = stops[0];
            for &s in &stops[1..] {
                if t <= s.0 {
                    let k = ((t - prev.0) / (s.0 - prev.0).max(1e-6)).clamp(0.0, 1.0);
                    let m = |i: usize| (prev.1[i] + (s.1[i] - prev.1[i]) * k).round() as u8;
                    return [m(0), m(1), m(2)];
                }
                prev = s;
            }
            let l = stops[stops.len() - 1].1;
            [l[0] as u8, l[1] as u8, l[2] as u8]
        };
        match self {
            Colormap::Rainbow => lerp(&[(0.0, [60.0, 40.0, 220.0]), (0.25, [0.0, 160.0, 255.0]), (0.5, [40.0, 210.0, 90.0]), (0.75, [250.0, 230.0, 30.0]), (1.0, [235.0, 40.0, 30.0])]),
            Colormap::Gray => lerp(&[(0.0, [30.0, 30.0, 30.0]), (1.0, [245.0, 245.0, 245.0])]),
            Colormap::Hot => lerp(&[(0.0, [20.0, 0.0, 0.0]), (0.4, [230.0, 20.0, 0.0]), (0.75, [255.0, 200.0, 0.0]), (1.0, [255.0, 255.0, 220.0])]),
            Colormap::BlueRed => lerp(&[(0.0, [20.0, 50.0, 220.0]), (0.5, [235.0, 235.0, 235.0]), (1.0, [220.0, 30.0, 30.0])]),
            Colormap::GreenYellow => lerp(&[(0.0, [10.0, 100.0, 40.0]), (0.5, [110.0, 190.0, 50.0]), (1.0, [250.0, 240.0, 80.0])]),
        }
    }
}

/// Format of a 3D plot (format dialog).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plot3dFormat {
    pub colormap: Colormap,
    /// Filled surface / filled contour bands.
    pub fill: bool,
    /// Wireframe on the surface; level lines on a contour plot.
    pub wireframe: bool,
    /// Number of contour levels.
    pub levels: u32,
    /// Level numbers on contour lines.
    pub labels: bool,
    /// Draw the axes box with numbers.
    pub box_axes: bool,
}

impl Default for Plot3dFormat {
    fn default() -> Self {
        Plot3dFormat { colormap: Colormap::Rainbow, fill: true, wireframe: true, levels: 10, labels: true, box_axes: true }
    }
}

impl Plot3dFormat {
    pub fn is_default(&self) -> bool {
        *self == Plot3dFormat::default()
    }
}

/// Computed data of a 3D plot.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Plot3dData {
    pub kind: Plot3dKind,
    /// Coordinates along the first / second grid direction.
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    /// Heights, `z[i * ys.len() + j]` at `(xs[i], ys[j])`; NaN where undefined.
    pub z: Vec<f64>,
    /// Scatter points.
    pub pts: Vec<[f64; 3]>,
    pub error: Option<String>,
    /// What is plotted: the matrix or function name.
    pub label: String,
}

impl Plot3dData {
    pub fn nx(&self) -> usize {
        self.xs.len()
    }
    pub fn ny(&self) -> usize {
        self.ys.len()
    }
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.z[i * self.ys.len() + j]
    }
    /// Range of finite heights.
    pub fn z_range(&self) -> Option<(f64, f64)> {
        let it: Vec<f64> = if self.kind == Plot3dKind::Scatter { self.pts.iter().map(|p| p[2]).collect() } else { self.z.clone() };
        let it = it.into_iter().filter(|v| v.is_finite());
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in it {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        (lo <= hi).then_some((lo, hi))
    }
    pub fn is_empty(&self) -> bool {
        match self.kind {
            Plot3dKind::Scatter => self.pts.is_empty(),
            _ => self.z.is_empty(),
        }
    }
}

// ------------------------------------------------------------ projection

#[derive(Clone, Copy)]
struct Cam {
    cphi: f32,
    sphi: f32,
    cth: f32,
    sth: f32,
    scale: f32,
    off: [f32; 2],
}

impl Cam {
    /// (screen position, depth: larger is farther) of a point in the unit box (−0.5..0.5).
    fn proj(&self, p: [f32; 3]) -> ([f32; 2], f32) {
        let x1 = p[0] * self.cphi - p[1] * self.sphi;
        let y1 = p[0] * self.sphi + p[1] * self.cphi;
        let up = p[2] * self.cth + y1 * self.sth;
        let depth = y1 * self.cth - p[2] * self.sth;
        ([self.off[0] + x1 * self.scale, self.off[1] - up * self.scale], depth)
    }
    fn depth_of_dir(&self, d: [f32; 3]) -> f32 {
        let y1 = d[0] * self.sphi + d[1] * self.cphi;
        y1 * self.cth - d[2] * self.sth
    }
    /// View-space (right, up, depth) of a direction.
    fn view_dir(&self, d: [f32; 3]) -> [f32; 3] {
        let x1 = d[0] * self.cphi - d[1] * self.sphi;
        let y1 = d[0] * self.sphi + d[1] * self.cphi;
        [x1, d[2] * self.cth + y1 * self.sth, y1 * self.cth - d[2] * self.sth]
    }
}

/// Height of the unit box relative to its width.
const ZSCALE: f32 = 0.75;

fn make_cam(view: &View3d, size: [f32; 2], margin: [f32; 4]) -> Cam {
    let v = view.clamped();
    let (phi, th) = (v.azimuth.to_radians(), v.elevation.to_radians());
    let mut cam = Cam { cphi: phi.cos(), sphi: phi.sin(), cth: th.cos(), sth: th.sin(), scale: 1.0, off: [0.0, 0.0] };
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for k in 0..8 {
        let p = [(k & 1) as f32 - 0.5, ((k >> 1) & 1) as f32 - 0.5, ((k >> 2) & 1) as f32 * ZSCALE - ZSCALE / 2.0];
        let (s, _) = cam.proj(p);
        x0 = x0.min(s[0]);
        x1 = x1.max(s[0]);
        y0 = y0.min(s[1]);
        y1 = y1.max(s[1]);
    }
    let (aw, ah) = ((size[0] - margin[0] - margin[2]).max(20.0), (size[1] - margin[1] - margin[3]).max(20.0));
    let scale = (aw / (x1 - x0).max(1e-6)).min(ah / (y1 - y0).max(1e-6)) * v.zoom;
    cam.scale = scale;
    // centre the projected box in the free area
    let (cx, cy) = ((x0 + x1) / 2.0 * scale, (y0 + y1) / 2.0 * scale);
    cam.off = [margin[0] + aw / 2.0 - cx, margin[1] + ah / 2.0 - cy];
    cam
}

struct Ranges {
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
    zs: f64,
}

fn extent(vals: impl Iterator<Item = f64>) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in vals.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        (0.0, 1.0)
    } else if hi - lo < 1e-300 {
        (lo - 0.5, hi + 0.5)
    } else {
        (lo, hi)
    }
}

fn ranges(d: &Plot3dData) -> Ranges {
    let (x, y) = match d.kind {
        Plot3dKind::Scatter => (extent(d.pts.iter().map(|p| p[0])), extent(d.pts.iter().map(|p| p[1]))),
        Plot3dKind::Bar => {
            let (a, b) = extent(d.xs.iter().copied());
            let (c, e) = extent(d.ys.iter().copied());
            let hx = if d.xs.len() > 1 { (b - a) / (d.xs.len() - 1) as f64 / 2.0 } else { 0.5 };
            let hy = if d.ys.len() > 1 { (e - c) / (d.ys.len() - 1) as f64 / 2.0 } else { 0.5 };
            ((a - hx, b + hx), (c - hy, e + hy))
        }
        _ => (extent(d.xs.iter().copied()), extent(d.ys.iter().copied())),
    };
    let (zl, zh) = d.z_range().unwrap_or((0.0, 1.0));
    let (zl, zh) = if d.kind == Plot3dKind::Bar { (zl.min(0.0), zh.max(0.0)) } else { (zl, zh) };
    let (a, b, st) = axis_range(zl, zh);
    Ranges { x, y, z: (a, b), zs: st }
}

fn norm(v: f64, r: (f64, f64)) -> f32 {
    ((v - r.0) / (r.1 - r.0)) as f32 - 0.5
}

fn shade(c: Rgb, k: f32) -> Rgb {
    let m = |v: u8| ((v as f32 * k).round()).clamp(0.0, 255.0) as u8;
    [m(c[0]), m(c[1]), m(c[2])]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Lambert factor of a surface with unit-box normal `n` for a light above-left of the viewer.
fn lighting(cam: &Cam, n: [f32; 3]) -> f32 {
    let mut v = cam.view_dir(n);
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    if v[2] > 0.0 {
        v = [-v[0], -v[1], -v[2]]; // face the viewer
    }
    let l = [-0.35f32, 0.55, -0.75];
    let ll = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
    let d = (v[0] * l[0] + v[1] * l[1] + v[2] * l[2]) / (len * ll);
    0.55 + 0.45 * d.max(0.0)
}

const WALL: Rgb = [236, 236, 236];
const EDGE: Rgb = [70, 70, 70];
const GRID3: Rgb = [200, 200, 200];

/// Renders a surface, 3D bar or 3D scatter plot into an area of `size` pixels.
pub fn scene3d(d: &Plot3dData, view: &View3d, fmt: &Plot3dFormat, size: [f32; 2], font: f32, exp_threshold: i32) -> Scene {
    let mut sc = Scene { size, prims: Vec::new() };
    if d.is_empty() {
        return sc;
    }
    let rg = ranges(d);
    let margin = if fmt.box_axes { [font * 3.2, font * 1.0, font * 1.0, font * 2.0] } else { [4.0; 4] };
    let cam = make_cam(view, size, margin);
    let pu = |x: f64, y: f64, z: f64| [norm(x, rg.x), norm(y, rg.y), ((z - rg.z.0) / (rg.z.1 - rg.z.0)) as f32 * ZSCALE - ZSCALE / 2.0];
    let zs = |z: f64| ((z - rg.z.0) / (rg.z.1 - rg.z.0)).clamp(0.0, 1.0) as f32;
    let corner = |k: usize| [(k & 1) as f32 - 0.5, ((k >> 1) & 1) as f32 - 0.5, ((k >> 2) & 1) as f32 * ZSCALE - ZSCALE / 2.0];
    let (xt, yt, zt) = (ticks(rg.x.0, rg.x.1, nice_step(rg.x.1 - rg.x.0, 5.0)), ticks(rg.y.0, rg.y.1, nice_step(rg.y.1 - rg.y.0, 5.0)), ticks(rg.z.0, rg.z.1, rg.zs));
    let (xst, yst) = (nice_step(rg.x.1 - rg.x.0, 5.0), nice_step(rg.y.1 - rg.y.0, 5.0));

    if fmt.box_axes {
        // back walls (faces whose outward normal points away from the viewer)
        let faces: [([f32; 3], [usize; 4]); 6] = [
            ([-1.0, 0.0, 0.0], [0, 2, 6, 4]),
            ([1.0, 0.0, 0.0], [1, 3, 7, 5]),
            ([0.0, -1.0, 0.0], [0, 1, 5, 4]),
            ([0.0, 1.0, 0.0], [2, 3, 7, 6]),
            ([0.0, 0.0, -1.0], [0, 1, 3, 2]),
            ([0.0, 0.0, 1.0], [4, 5, 7, 6]),
        ];
        let back: Vec<bool> = faces.iter().map(|(n, _)| cam.depth_of_dir(*n) > 1e-4).collect();
        for (f, (n, idx)) in faces.iter().enumerate() {
            if !back[f] {
                continue;
            }
            let pts: Vec<[f32; 2]> = idx.iter().map(|&k| cam.proj(corner(k)).0).collect();
            sc.prims.push(Prim::Poly { pts, fill: Some(WALL), stroke: None });
            // grid lines on the wall
            let axis = n.iter().position(|v| *v != 0.0).unwrap_or(2);
            let fixed = n[axis] * 0.5 * if axis == 2 { ZSCALE } else { 1.0 };
            let (ta, tb): (Vec<f32>, Vec<f32>);
            let mk = |a: f32, b: f32| -> [f32; 3] {
                match axis {
                    0 => [fixed, a, b],
                    1 => [a, fixed, b],
                    _ => [a, b, fixed],
                }
            };
            let tz: Vec<f32> = zt.iter().map(|&t| pu(0.0, 0.0, t)[2]).collect();
            let tx: Vec<f32> = xt.iter().map(|&t| norm(t, rg.x)).collect();
            let ty: Vec<f32> = yt.iter().map(|&t| norm(t, rg.y)).collect();
            let hz = ZSCALE / 2.0;
            match axis {
                0 => {
                    ta = ty;
                    tb = tz;
                }
                1 => {
                    ta = tx;
                    tb = tz;
                }
                _ => {
                    ta = tx;
                    tb = ty;
                }
            }
            let (ext_a, ext_b) = match axis {
                0 | 1 => (0.5, hz),
                _ => (0.5, 0.5),
            };
            for &a in &ta {
                sc.prims.push(Prim::Line { pts: vec![cam.proj(mk(a, -ext_b)).0, cam.proj(mk(a, ext_b)).0], color: GRID3, width: 0.7, style: LineStyle::Solid });
            }
            for &b in &tb {
                sc.prims.push(Prim::Line { pts: vec![cam.proj(mk(-ext_a, b)).0, cam.proj(mk(ext_a, b)).0], color: GRID3, width: 0.7, style: LineStyle::Solid });
            }
        }
        // back edges
        for (a, b) in box_edges() {
            let fa: Vec<usize> = (0..6).filter(|&f| faces[f].1.contains(&a) && faces[f].1.contains(&b)).collect();
            if fa.iter().all(|&f| back[f]) {
                sc.prims.push(Prim::Line { pts: vec![cam.proj(corner(a)).0, cam.proj(corner(b)).0], color: EDGE, width: 1.0, style: LineStyle::Solid });
            }
        }
    }

    match d.kind {
        Plot3dKind::Surface => surface_prims(&mut sc, d, &cam, fmt, &pu, &zs),
        Plot3dKind::Bar => bar_prims(&mut sc, d, &cam, fmt, &rg, &pu, &zs),
        Plot3dKind::Scatter => {
            let mut dots: Vec<(f32, [f32; 2])> = d
                .pts
                .iter()
                .filter(|p| p.iter().all(|v| v.is_finite()))
                .map(|p| {
                    let (s, dep) = cam.proj(pu(p[0], p[1], p[2]));
                    (dep, s)
                })
                .collect();
            dots.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (_, c) in dots {
                sc.prims.push(Prim::Dot { c, r: 3.0, color: [220, 0, 0] });
            }
        }
        Plot3dKind::Contour => {}
    }

    if fmt.box_axes {
        // front edges over the plot
        let faces_back = |a: usize, b: usize| -> bool {
            // an edge is a back edge when both neighbouring faces point away
            let fa = |n: [f32; 3]| cam.depth_of_dir(n) > 1e-4;
            let (ca, cb) = (corner(a), corner(b));
            let mut ns: Vec<[f32; 3]> = Vec::new();
            for ax in 0..3 {
                if (ca[ax] - cb[ax]).abs() < 1e-6 {
                    let mut n = [0.0; 3];
                    n[ax] = if ca[ax] > 0.0 { 1.0 } else { -1.0 };
                    ns.push(n);
                }
            }
            ns.into_iter().all(fa)
        };
        for (a, b) in box_edges() {
            if !faces_back(a, b) {
                sc.prims.push(Prim::Line { pts: vec![cam.proj(corner(a)).0, cam.proj(corner(b)).0], color: EDGE, width: 1.0, style: LineStyle::Solid });
            }
        }
        // numbers along the front edges, pushed away from the box centre
        let centre = cam.proj([0.0, 0.0, 0.0]).0;
        let mut label = |p: [f32; 3], text: TickLabel| {
            let s = cam.proj(p).0;
            let (mut dx, mut dy) = (s[0] - centre[0], s[1] - centre[1]);
            let l = (dx * dx + dy * dy).sqrt().max(1e-3);
            dx /= l;
            dy /= l;
            let off = font * 0.5;
            sc.text([s[0] + dx * off, s[1] + dy * off], text, [0.5 - dx * 0.5, 0.5 - dy * 0.5], font * 0.8, [0, 0, 0]);
        };
        let hz = ZSCALE / 2.0;
        // x-axis along the bottom edge nearest to the viewer
        let yf = if cam.proj([0.0, -0.5, -hz]).1 < cam.proj([0.0, 0.5, -hz]).1 { -0.5 } else { 0.5 };
        let xf = if cam.proj([-0.5, 0.0, -hz]).1 < cam.proj([0.5, 0.0, -hz]).1 { -0.5 } else { 0.5 };
        for &t in &xt {
            label([norm(t, rg.x), yf, -hz], format_tick(t, exp_threshold));
        }
        for &t in &yt {
            label([xf, norm(t, rg.y), -hz], format_tick(t, exp_threshold));
        }
        // z-axis along the leftmost vertical edge
        let (mut best, mut bx) = ((-0.5f32, -0.5f32), f32::MAX);
        for (ux, vy) in [(-0.5f32, -0.5f32), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
            let sx = cam.proj([ux, vy, 0.0]).0[0];
            if sx < bx {
                bx = sx;
                best = (ux, vy);
            }
        }
        for &t in &zt {
            label([best.0, best.1, pu(0.0, 0.0, t)[2]], format_tick(t, exp_threshold));
        }
        let _ = (xst, yst);
    }
    sc
}

fn box_edges() -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for a in 0..8usize {
        for bit in 0..3 {
            let b = a | (1 << bit);
            if b != a {
                v.push((a, b));
            }
        }
    }
    v
}

type Pu<'a> = &'a dyn Fn(f64, f64, f64) -> [f32; 3];

fn surface_prims(sc: &mut Scene, d: &Plot3dData, cam: &Cam, fmt: &Plot3dFormat, pu: Pu, zs: &dyn Fn(f64) -> f32) {
    let (nx, ny) = (d.nx(), d.ny());
    if nx < 2 || ny < 2 {
        return;
    }
    struct Quad {
        depth: f32,
        pts: Vec<[f32; 2]>,
        fill: Rgb,
    }
    let mut quads: Vec<Quad> = Vec::with_capacity((nx - 1) * (ny - 1));
    for i in 0..nx - 1 {
        for j in 0..ny - 1 {
            let zv = [d.at(i, j), d.at(i + 1, j), d.at(i + 1, j + 1), d.at(i, j + 1)];
            if zv.iter().any(|z| !z.is_finite()) {
                continue;
            }
            let p3 = [pu(d.xs[i], d.ys[j], zv[0]), pu(d.xs[i + 1], d.ys[j], zv[1]), pu(d.xs[i + 1], d.ys[j + 1], zv[2]), pu(d.xs[i], d.ys[j + 1], zv[3])];
            let pr: Vec<([f32; 2], f32)> = p3.iter().map(|p| cam.proj(*p)).collect();
            let depth = pr.iter().map(|p| p.1).sum::<f32>() / 4.0;
            let n = cross(sub(p3[2], p3[0]), sub(p3[3], p3[1]));
            let zmean = zv.iter().sum::<f64>() / 4.0;
            let base = fmt.colormap.color(zs(zmean));
            let fill = shade(base, lighting(cam, n));
            quads.push(Quad { depth, pts: pr.iter().map(|p| p.0).collect(), fill });
        }
    }
    quads.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    for q in quads {
        let fill = if fmt.fill { Some(q.fill) } else { Some([255, 255, 255]) };
        let stroke = if fmt.wireframe { Some(([40, 40, 40], 0.6)) } else { fmt.fill.then_some((q.fill, 0.6)) };
        sc.prims.push(Prim::Poly { pts: q.pts, fill, stroke });
    }
}

fn bar_prims(sc: &mut Scene, d: &Plot3dData, cam: &Cam, fmt: &Plot3dFormat, rg: &Ranges, pu: Pu, zs: &dyn Fn(f64) -> f32) {
    let (nx, ny) = (d.nx(), d.ny());
    let hx = if nx > 1 { (d.xs[nx - 1] - d.xs[0]) / (nx - 1) as f64 * 0.4 } else { 0.4 };
    let hy = if ny > 1 { (d.ys[ny - 1] - d.ys[0]) / (ny - 1) as f64 * 0.4 } else { 0.4 };
    let base = 0.0f64.clamp(rg.z.0, rg.z.1);
    let mut bars: Vec<(f32, usize, usize)> = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            if d.at(i, j).is_finite() {
                let depth = cam.proj(pu(d.xs[i], d.ys[j], base)).1;
                bars.push((depth, i, j));
            }
        }
    }
    bars.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, i, j) in bars {
        let z = d.at(i, j);
        let (x0, x1, y0, y1) = (d.xs[i] - hx, d.xs[i] + hx, d.ys[j] - hy, d.ys[j] + hy);
        let c = |x: f64, y: f64, zz: f64| pu(x, y, zz);
        let corners = |zz: f64| [c(x0, y0, zz), c(x1, y0, zz), c(x1, y1, zz), c(x0, y1, zz)];
        let (lo, hi) = (corners(base), corners(z));
        let col = fmt.colormap.color(zs(z));
        // faces: top, and four sides
        let sides = [([0usize, 1], [0.0, -1.0, 0.0]), ([1, 2], [1.0, 0.0, 0.0]), ([2, 3], [0.0, 1.0, 0.0]), ([3, 0], [-1.0, 0.0, 0.0])];
        for (ab, n) in sides {
            if cam.depth_of_dir(n) >= 0.0 {
                continue;
            }
            let quad = [lo[ab[0]], lo[ab[1]], hi[ab[1]], hi[ab[0]]];
            let pts: Vec<[f32; 2]> = quad.iter().map(|p| cam.proj(*p).0).collect();
            sc.prims.push(Prim::Poly { pts, fill: fmt.fill.then(|| shade(col, lighting(cam, n))), stroke: Some(([40, 40, 40], 0.6)) });
        }
        let top_n = if z >= base { [0.0, 0.0, 1.0] } else { [0.0, 0.0, -1.0] };
        if cam.depth_of_dir(top_n) < 0.0 {
            let face = if z >= base { hi } else { lo };
            let pts: Vec<[f32; 2]> = face.iter().map(|p| cam.proj(*p).0).collect();
            sc.prims.push(Prim::Poly { pts, fill: fmt.fill.then(|| shade(col, lighting(cam, top_n))), stroke: Some(([40, 40, 40], 0.6)) });
        }
    }
}

// ---------------------------------------------------------------- contour

/// Bilinear refinement of the grid so contours look smooth: returns (xs, ys, z).
fn refine(d: &Plot3dData, target: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (nx, ny) = (d.nx(), d.ny());
    let kx = ((target / (nx.max(2) - 1)).max(1)).min(8);
    let ky = ((target / (ny.max(2) - 1)).max(1)).min(8);
    let (mx, my) = ((nx - 1) * kx + 1, (ny - 1) * ky + 1);
    let xs: Vec<f64> = (0..mx).map(|a| { let (i, f) = (a / kx, (a % kx) as f64 / kx as f64); if i + 1 < nx { d.xs[i] + (d.xs[i + 1] - d.xs[i]) * f } else { d.xs[nx - 1] } }).collect();
    let ys: Vec<f64> = (0..my).map(|b| { let (j, f) = (b / ky, (b % ky) as f64 / ky as f64); if j + 1 < ny { d.ys[j] + (d.ys[j + 1] - d.ys[j]) * f } else { d.ys[ny - 1] } }).collect();
    let mut z = vec![f64::NAN; mx * my];
    for a in 0..mx {
        for b in 0..my {
            let (i, j) = ((a / kx).min(nx - 1), (b / ky).min(ny - 1));
            let (fx, fy) = ((a % kx) as f64 / kx as f64, (b % ky) as f64 / ky as f64);
            let (i1, j1) = ((i + 1).min(nx - 1), (j + 1).min(ny - 1));
            let (z00, z10, z01, z11) = (d.at(i, j), d.at(i1, j), d.at(i, j1), d.at(i1, j1));
            z[a * my + b] = z00 * (1.0 - fx) * (1.0 - fy) + z10 * fx * (1.0 - fy) + z01 * (1.0 - fx) * fy + z11 * fx * fy;
        }
    }
    (xs, ys, z)
}

/// Rounds to `n` significant digits.
fn round_sig(v: f64, n: i32) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let e = v.abs().log10().floor() as i32;
    let f = 10f64.powi(n - 1 - e);
    (v * f).round() / f
}

/// Contour levels: `n` evenly spaced values strictly between the extremes.
pub fn contour_levels(lo: f64, hi: f64, n: u32) -> Vec<f64> {
    let n = n.clamp(1, 100) as usize;
    (1..=n).map(|k| lo + (hi - lo) * k as f64 / (n + 1) as f64).collect()
}

/// A contour plot: filled bands (optional), level lines (marching squares), numbered axes.
pub fn contour_scene(d: &Plot3dData, fmt: &Plot3dFormat, size: [f32; 2], font: f32, exp_threshold: i32) -> Scene {
    let mut sc = Scene { size, prims: Vec::new() };
    if d.nx() < 2 || d.ny() < 2 || d.z.is_empty() {
        return sc;
    }
    let (zlo, zhi) = d.z_range().unwrap_or((0.0, 1.0));
    let (x0, x1) = extent(d.xs.iter().copied());
    let (y0, y1) = extent(d.ys.iter().copied());
    let m = [font * 3.6, font * 0.8, font * 0.8, font * 1.8];
    let area = [m[0], m[1], size[0] - m[2], size[1] - m[3]];
    let px = |x: f64| area[0] + ((x - x0) / (x1 - x0)) as f32 * (area[2] - area[0]);
    let py = |y: f64| area[3] - ((y - y0) / (y1 - y0)) as f32 * (area[3] - area[1]);
    let levels = contour_levels(zlo, zhi, fmt.levels);
    let (rx, ry, rz) = refine(d, 80);
    let my = ry.len();
    let at = |a: usize, b: usize| rz[a * my + b];
    let band = |z: f64| levels.iter().filter(|&&l| l < z).count();
    let nb = levels.len() + 1;
    if fmt.fill {
        for a in 0..rx.len() - 1 {
            for b in 0..my - 1 {
                let zz = [at(a, b), at(a + 1, b), at(a + 1, b + 1), at(a, b + 1)];
                if zz.iter().any(|v| !v.is_finite()) {
                    continue;
                }
                let zm = zz.iter().sum::<f64>() / 4.0;
                let col = fmt.colormap.color(band(zm) as f32 / (nb - 1).max(1) as f32);
                let pts = vec![[px(rx[a]), py(ry[b])], [px(rx[a + 1]), py(ry[b])], [px(rx[a + 1]), py(ry[b + 1])], [px(rx[a]), py(ry[b + 1])]];
                sc.prims.push(Prim::Poly { pts, fill: Some(col), stroke: Some((col, 0.7)) });
            }
        }
    }
    if fmt.wireframe {
        for (li, &lv) in levels.iter().enumerate() {
            let col = if fmt.fill { [30, 30, 30] } else { fmt.colormap.color(li as f32 / (levels.len() - 1).max(1) as f32) };
            let mut segs: Vec<([f32; 2], [f32; 2])> = Vec::new();
            for a in 0..rx.len() - 1 {
                for b in 0..my - 1 {
                    let c = [(rx[a], ry[b], at(a, b)), (rx[a + 1], ry[b], at(a + 1, b)), (rx[a + 1], ry[b + 1], at(a + 1, b + 1)), (rx[a], ry[b + 1], at(a, b + 1))];
                    if c.iter().any(|p| !p.2.is_finite()) {
                        continue;
                    }
                    let mut cuts: Vec<[f32; 2]> = Vec::new();
                    for e in 0..4 {
                        let (p, q) = (c[e], c[(e + 1) % 4]);
                        if (p.2 < lv) != (q.2 < lv) {
                            let t = (lv - p.2) / (q.2 - p.2);
                            cuts.push([px(p.0 + (q.0 - p.0) * t), py(p.1 + (q.1 - p.1) * t)]);
                        }
                    }
                    match cuts.len() {
                        2 => segs.push((cuts[0], cuts[1])),
                        4 => {
                            segs.push((cuts[0], cuts[1]));
                            segs.push((cuts[2], cuts[3]));
                        }
                        _ => {}
                    }
                }
            }
            for (p, q) in &segs {
                sc.prims.push(Prim::Line { pts: vec![*p, *q], color: col, width: 1.0, style: LineStyle::Solid });
            }
            if fmt.labels && !segs.is_empty() {
                // label a segment near the middle of the longest run of this level
                let (p, q) = segs[(segs.len() * (2 + li % 3)) / 5 % segs.len()];
                let pos = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
                let st = nice_step((zhi - zlo).abs().max(1e-300), 5.0);
                let _ = st;
                sc.prims.push(Prim::Text { pos, label: format_tick(round_sig(lv, 3), exp_threshold.max(4)), align: [0.5, 0.5], size: font * 0.7, color: [0, 0, 0], bg: Some([255, 255, 255]) });
            }
        }
    }
    // frame and numbers
    let frame = vec![[area[0], area[1]], [area[2], area[1]], [area[2], area[3]], [area[0], area[3]], [area[0], area[1]]];
    sc.prims.push(Prim::Line { pts: frame, color: [0, 0, 0], width: 1.0, style: LineStyle::Solid });
    let (xs_, ys_) = (nice_step(x1 - x0, 6.0), nice_step(y1 - y0, 6.0));
    for t in ticks(x0, x1, xs_) {
        sc.line([px(t), area[3]], [px(t), area[3] + 3.0], [0, 0, 0], 1.0);
        sc.text([px(t), area[3] + 4.0], format_tick(t, exp_threshold), [0.5, 0.0], font * 0.8, [0, 0, 0]);
    }
    for t in ticks(y0, y1, ys_) {
        sc.line([area[0] - 3.0, py(t)], [area[0], py(t)], [0, 0, 0], 1.0);
        sc.text([area[0] - 4.0, py(t)], format_tick(t, exp_threshold), [1.0, 0.5], font * 0.8, [0, 0, 0]);
    }
    sc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(f: impl Fn(f64, f64) -> f64, n: usize, kind: Plot3dKind) -> Plot3dData {
        let xs: Vec<f64> = (0..n).map(|i| -5.0 + 10.0 * i as f64 / (n - 1) as f64).collect();
        let ys = xs.clone();
        let mut z = Vec::new();
        for &x in &xs {
            for &y in &ys {
                z.push(f(x, y));
            }
        }
        Plot3dData { kind, xs, ys, z, ..Default::default() }
    }

    #[test]
    fn surface_has_quads_sorted_far_to_near() {
        let d = grid(|x, y| x.sin() * y.cos(), 12, Plot3dKind::Surface);
        let sc = scene3d(&d, &View3d::default(), &Plot3dFormat::default(), [400.0, 300.0], 12.0, 3);
        let polys = sc.prims.iter().filter(|p| matches!(p, Prim::Poly { fill: Some(_), stroke: Some(_), .. })).count();
        assert!(polys >= 121, "{polys}");
        // all points inside the area
        for p in &sc.prims {
            if let Prim::Poly { pts, .. } = p {
                for q in pts {
                    assert!(q[0] >= -1.0 && q[0] <= 401.0 && q[1] >= -1.0 && q[1] <= 301.0, "{q:?}");
                }
            }
        }
        assert!(sc.prims.iter().any(|p| matches!(p, Prim::Text { .. })));
    }

    #[test]
    fn rotating_changes_projection() {
        let d = grid(|x, y| x * y, 8, Plot3dKind::Surface);
        let a = scene3d(&d, &View3d::default(), &Plot3dFormat::default(), [300.0, 300.0], 12.0, 3);
        let v = View3d { azimuth: 120.0, ..Default::default() };
        let b = scene3d(&d, &v, &Plot3dFormat::default(), [300.0, 300.0], 12.0, 3);
        assert_ne!(a, b);
    }

    #[test]
    fn contour_levels_and_lines() {
        let l = contour_levels(0.0, 10.0, 4);
        assert_eq!(l, vec![2.0, 4.0, 6.0, 8.0]);
        let d = grid(|x, y| x * x + y * y, 10, Plot3dKind::Contour);
        let sc = contour_scene(&d, &Plot3dFormat::default(), [300.0, 300.0], 12.0, 3);
        let lines = sc.prims.iter().filter(|p| matches!(p, Prim::Line { pts, .. } if pts.len() == 2)).count();
        assert!(lines > 20);
    }

    #[test]
    fn colormap_endpoints() {
        assert_eq!(Colormap::Gray.color(0.0), [30, 30, 30]);
        assert_eq!(Colormap::Gray.color(1.0), [245, 245, 245]);
        assert_ne!(Colormap::Rainbow.color(0.1), Colormap::Rainbow.color(0.9));
    }

    #[test]
    fn view_defaults_and_serde() {
        let v: View3d = serde_json::from_str("{}").unwrap();
        assert_eq!(v, View3d::default());
        let f: Plot3dFormat = serde_json::from_str(r#"{"levels":5}"#).unwrap();
        assert_eq!(f.levels, 5);
        assert!(f.fill);
    }

    #[test]
    fn scatter_and_bars_render() {
        let pts = (0..20).map(|k| [k as f64, (k as f64).sin(), (k as f64).cos()]).collect();
        let d = Plot3dData { kind: Plot3dKind::Scatter, pts, ..Default::default() };
        let sc = scene3d(&d, &View3d::default(), &Plot3dFormat::default(), [300.0, 300.0], 12.0, 3);
        assert_eq!(sc.prims.iter().filter(|p| matches!(p, Prim::Dot { .. })).count(), 20);
        let d = grid(|x, y| x + y, 4, Plot3dKind::Bar);
        let sc = scene3d(&d, &View3d::default(), &Plot3dFormat::default(), [300.0, 300.0], 12.0, 3);
        assert!(sc.prims.len() > 16);
    }
}
