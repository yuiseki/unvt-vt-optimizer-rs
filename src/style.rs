use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

const PAINT_PROPERTIES_TO_CHECK: &[&str] = &[
    "fill-opacity",
    "fill-outline-color",
    "line-opacity",
    "line-width",
    "icon-size",
    "text-size",
    "text-max-width",
    "text-opacity",
    "raster-opacity",
    "circle-radius",
    "circle-opacity",
    "fill-extrusion-opacity",
    "heatmap-opacity",
];

#[derive(Debug, Clone)]
enum PaintValue {
    Number(f64),
    Stops(Vec<(u8, f64)>),
}

impl PaintValue {
    fn is_nonzero_at_zoom(&self, zoom: u8) -> bool {
        match self {
            PaintValue::Number(value) => *value != 0.0,
            PaintValue::Stops(stops) => {
                if let Some((_, value)) = stops.iter().find(|(z, _)| *z == zoom) {
                    *value != 0.0
                } else {
                    true
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MapboxStyleLayer {
    minzoom: Option<f64>,
    maxzoom: Option<f64>,
    visibility: Option<String>,
    paint: HashMap<String, PaintValue>,
    filter: Option<Filter>,
}

impl MapboxStyleLayer {
    fn is_visible_on_zoom(&self, zoom: u8) -> bool {
        self.check_layout_visibility() && self.check_zoom_underflow(zoom) && self.check_zoom_overflow(zoom)
    }

    fn check_layout_visibility(&self) -> bool {
        match self.visibility.as_deref() {
            Some("none") => false,
            _ => true,
        }
    }

    fn check_zoom_underflow(&self, zoom: u8) -> bool {
        self.minzoom.map_or(true, |minzoom| (zoom as f64) >= minzoom)
    }

    fn check_zoom_overflow(&self, zoom: u8) -> bool {
        self.maxzoom.map_or(true, |maxzoom| maxzoom > (zoom as f64))
    }

    fn is_rendered(&self, zoom: u8) -> bool {
        for prop in PAINT_PROPERTIES_TO_CHECK {
            if !self.check_paint_property_not_zero(prop, zoom) {
                return false;
            }
        }
        true
    }

    fn check_paint_property_not_zero(&self, property: &str, zoom: u8) -> bool {
        match self.paint.get(property) {
            Some(value) => value.is_nonzero_at_zoom(zoom),
            None => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapboxStyle {
    layers_by_source_layer: HashMap<String, Vec<MapboxStyleLayer>>,
}

impl MapboxStyle {
    pub fn source_layers(&self) -> HashSet<String> {
        self.layers_by_source_layer.keys().cloned().collect()
    }

    pub fn is_layer_visible_on_zoom(&self, layer_name: &str, zoom: u8) -> bool {
        self.layers_by_source_layer
            .get(layer_name)
            .map(|layers| {
                layers.iter().any(|layer| {
                    layer.is_visible_on_zoom(zoom) && layer.is_rendered(zoom)
                })
            })
            .unwrap_or(false)
    }

    pub fn should_keep_feature(
        &self,
        layer_name: &str,
        zoom: u8,
        feature: &mvt_reader::feature::Feature,
    ) -> FilterResult {
        let Some(layers) = self.layers_by_source_layer.get(layer_name) else {
            return FilterResult::False;
        };
        let mut saw_unknown = false;
        for layer in layers {
            if !layer.is_visible_on_zoom(zoom) || !layer.is_rendered(zoom) {
                continue;
            }
            let result = match layer.filter.as_ref() {
                None => FilterResult::True,
                Some(filter) => filter.evaluate(feature),
            };
            match result {
                FilterResult::True => return FilterResult::True,
                FilterResult::Unknown => saw_unknown = true,
                FilterResult::False => {}
            }
        }
        if saw_unknown {
            FilterResult::Unknown
        } else {
            FilterResult::False
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterResult {
    True,
    False,
    Unknown,
}

#[derive(Debug, Clone)]
enum FilterValue {
    String(String),
    Number(f64),
    Bool(bool),
}

impl FilterValue {
    fn equals(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::String(a), FilterValue::String(b)) => a == b,
            (FilterValue::Number(a), FilterValue::Number(b)) => (*a - *b).abs() < f64::EPSILON,
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
enum Filter {
    Eq(String, FilterValue),
    Neq(String, FilterValue),
    In(String, Vec<FilterValue>),
    NotIn(String, Vec<FilterValue>),
    Has(String),
    NotHas(String),
    All(Vec<Filter>),
    Any(Vec<Filter>),
    None(Vec<Filter>),
    Unknown,
}

impl Filter {
    fn evaluate(&self, feature: &mvt_reader::feature::Feature) -> FilterResult {
        match self {
            Filter::Eq(key, value) => match feature_value(feature, key) {
                Some(actual) => FilterResult::from_bool(actual.equals(value)),
                None => FilterResult::Unknown,
            },
            Filter::Neq(key, value) => match feature_value(feature, key) {
                Some(actual) => FilterResult::from_bool(!actual.equals(value)),
                None => FilterResult::Unknown,
            },
            Filter::In(key, values) => match feature_value(feature, key) {
                Some(actual) => FilterResult::from_bool(values.iter().any(|v| actual.equals(v))),
                None => FilterResult::Unknown,
            },
            Filter::NotIn(key, values) => match feature_value(feature, key) {
                Some(actual) => FilterResult::from_bool(!values.iter().any(|v| actual.equals(v))),
                None => FilterResult::Unknown,
            },
            Filter::Has(key) => FilterResult::from_bool(feature_has(feature, key)),
            Filter::NotHas(key) => FilterResult::from_bool(!feature_has(feature, key)),
            Filter::All(filters) => {
                let mut saw_unknown = false;
                for filter in filters {
                    match filter.evaluate(feature) {
                        FilterResult::True => {}
                        FilterResult::False => return FilterResult::False,
                        FilterResult::Unknown => saw_unknown = true,
                    }
                }
                if saw_unknown {
                    FilterResult::Unknown
                } else {
                    FilterResult::True
                }
            }
            Filter::Any(filters) => {
                let mut saw_unknown = false;
                for filter in filters {
                    match filter.evaluate(feature) {
                        FilterResult::True => return FilterResult::True,
                        FilterResult::False => {}
                        FilterResult::Unknown => saw_unknown = true,
                    }
                }
                if saw_unknown {
                    FilterResult::Unknown
                } else {
                    FilterResult::False
                }
            }
            Filter::None(filters) => {
                let mut saw_unknown = false;
                for filter in filters {
                    match filter.evaluate(feature) {
                        FilterResult::True => return FilterResult::False,
                        FilterResult::False => {}
                        FilterResult::Unknown => saw_unknown = true,
                    }
                }
                if saw_unknown {
                    FilterResult::Unknown
                } else {
                    FilterResult::True
                }
            }
            Filter::Unknown => FilterResult::Unknown,
        }
    }
}

impl FilterResult {
    fn from_bool(value: bool) -> Self {
        if value {
            FilterResult::True
        } else {
            FilterResult::False
        }
    }
}

fn feature_has(feature: &mvt_reader::feature::Feature, key: &str) -> bool {
    if key == "$type" {
        return true;
    }
    feature
        .properties
        .as_ref()
        .map(|props| props.contains_key(key))
        .unwrap_or(false)
}

fn feature_value(feature: &mvt_reader::feature::Feature, key: &str) -> Option<FilterValue> {
    if key == "$type" {
        return Some(FilterValue::String(feature_type(feature).to_string()));
    }
    let props = feature.properties.as_ref()?;
    let value = props.get(key)?;
    match value {
        mvt_reader::feature::Value::String(text) => Some(FilterValue::String(text.clone())),
        mvt_reader::feature::Value::Float(val) => Some(FilterValue::Number(*val as f64)),
        mvt_reader::feature::Value::Double(val) => Some(FilterValue::Number(*val)),
        mvt_reader::feature::Value::Int(val) => Some(FilterValue::Number(*val as f64)),
        mvt_reader::feature::Value::UInt(val) => Some(FilterValue::Number(*val as f64)),
        mvt_reader::feature::Value::SInt(val) => Some(FilterValue::Number(*val as f64)),
        mvt_reader::feature::Value::Bool(val) => Some(FilterValue::Bool(*val)),
        mvt_reader::feature::Value::Null => None,
    }
}

fn feature_type(feature: &mvt_reader::feature::Feature) -> &'static str {
    use geo_types::Geometry;
    match feature.geometry {
        Geometry::Point(_) | Geometry::MultiPoint(_) => "Point",
        Geometry::LineString(_) | Geometry::MultiLineString(_) | Geometry::Line(_) => "LineString",
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) | Geometry::Rect(_) | Geometry::Triangle(_) => "Polygon",
        Geometry::GeometryCollection(_) => "Unknown",
    }
}

fn parse_paint_value(value: &Value) -> Option<PaintValue> {
    if let Some(number) = value.as_f64() {
        return Some(PaintValue::Number(number));
    }
    let stops = value.get("stops")?.as_array()?;
    let mut parsed = Vec::new();
    for stop in stops {
        let arr = stop.as_array()?;
        if arr.len() < 2 {
            continue;
        }
        let zoom = arr[0].as_f64()? as i64;
        let value = arr[1].as_f64()?;
        if !(0..=255).contains(&zoom) {
            continue;
        }
        parsed.push((zoom as u8, value));
    }
    if parsed.is_empty() {
        None
    } else {
        Some(PaintValue::Stops(parsed))
    }
}

fn parse_filter(value: &Value) -> Option<Filter> {
    let array = value.as_array()?;
    if array.is_empty() {
        return None;
    }
    let op = array[0].as_str()?;
    match op {
        "==" | "!=" => {
            if array.len() < 3 {
                return Some(Filter::Unknown);
            }
            let key = array[1].as_str()?.to_string();
            let value = parse_filter_value(&array[2])?;
            if op == "==" {
                Some(Filter::Eq(key, value))
            } else {
                Some(Filter::Neq(key, value))
            }
        }
        "in" | "!in" => {
            if array.len() < 3 {
                return Some(Filter::Unknown);
            }
            let key = array[1].as_str()?.to_string();
            let mut values = Vec::new();
            if let Some(list) = array[2].as_array() {
                for item in list {
                    if let Some(value) = parse_filter_value(item) {
                        values.push(value);
                    } else {
                        return Some(Filter::Unknown);
                    }
                }
            } else {
                for item in &array[2..] {
                    if let Some(value) = parse_filter_value(item) {
                        values.push(value);
                    } else {
                        return Some(Filter::Unknown);
                    }
                }
            }
            if op == "in" {
                Some(Filter::In(key, values))
            } else {
                Some(Filter::NotIn(key, values))
            }
        }
        "has" | "!has" => {
            if array.len() < 2 {
                return Some(Filter::Unknown);
            }
            let key = array[1].as_str()?.to_string();
            if op == "has" {
                Some(Filter::Has(key))
            } else {
                Some(Filter::NotHas(key))
            }
        }
        "all" | "any" | "none" => {
            let mut filters = Vec::new();
            for item in &array[1..] {
                if let Some(filter) = parse_filter(item) {
                    filters.push(filter);
                } else {
                    filters.push(Filter::Unknown);
                }
            }
            match op {
                "all" => Some(Filter::All(filters)),
                "any" => Some(Filter::Any(filters)),
                _ => Some(Filter::None(filters)),
            }
        }
        _ => Some(Filter::Unknown),
    }
}

fn parse_filter_value(value: &Value) -> Option<FilterValue> {
    if let Some(text) = value.as_str() {
        return Some(FilterValue::String(text.to_string()));
    }
    if let Some(number) = value.as_f64() {
        return Some(FilterValue::Number(number));
    }
    if let Some(boolean) = value.as_bool() {
        return Some(FilterValue::Bool(boolean));
    }
    None
}

pub fn read_style(path: &Path) -> Result<MapboxStyle> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read style file: {}", path.display()))?;
    let value: Value = serde_json::from_str(&contents).context("parse style json")?;
    let layers = value
        .get("layers")
        .and_then(|layers| layers.as_array())
        .ok_or_else(|| anyhow::anyhow!("style json missing layers array"))?;

    let mut layers_by_source_layer: HashMap<String, Vec<MapboxStyleLayer>> = HashMap::new();
    for layer in layers {
        if layer.get("source").is_none() {
            continue;
        }
        let Some(source_layer) = layer.get("source-layer").and_then(|v| v.as_str()) else {
            continue;
        };
        let minzoom = layer.get("minzoom").and_then(|v| v.as_f64());
        let maxzoom = layer.get("maxzoom").and_then(|v| v.as_f64());
        let visibility = layer
            .get("layout")
            .and_then(|layout| layout.get("visibility"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let mut paint = HashMap::new();
        if let Some(props) = layer.get("paint").and_then(|paint| paint.as_object()) {
            for (key, value) in props {
                if let Some(parsed) = parse_paint_value(value) {
                    paint.insert(key.clone(), parsed);
                }
            }
        }
        let filter = layer.get("filter").and_then(parse_filter);
        layers_by_source_layer
            .entry(source_layer.to_string())
            .or_default()
            .push(MapboxStyleLayer {
                minzoom,
                maxzoom,
                visibility,
                paint,
                filter,
            });
    }

    if layers_by_source_layer.is_empty() {
        anyhow::bail!("style json contains no source-layer entries");
    }
    Ok(MapboxStyle {
        layers_by_source_layer,
    })
}

pub fn read_style_source_layers(path: &Path) -> Result<HashSet<String>> {
    Ok(read_style(path)?.source_layers())
}
