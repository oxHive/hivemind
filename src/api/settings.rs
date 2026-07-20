use super::*;

// --- sync settings (read-only from file in v1) ---

pub(super) async fn get_sync_settings(Extension(sync): Extension<SyncSettings>) -> Json<Value> {
    Json(json!({
        "enabled": sync.enabled,
        "remote_url": sync.remote_url,
        "interval_seconds": sync.interval_seconds,
        "sync_on_store": sync.sync_on_store,
        "sync_on_startup": sync.sync_on_startup,
    }))
}

pub(super) async fn save_sync_settings(Json(_): Json<Value>) -> Json<Value> {
    Json(
        json!({ "saved": false, "message": "Sync settings are managed via config.toml — restart hivemind after editing." }),
    )
}

fn default_tag_namespaces() -> Value {
    json!({
        "project": { "color": "#4a9eff", "values": [], "single_value": true },
        "lang": { "color": "#e0607e", "values": [] },
        "area": { "color": "#5fb8b0", "values": [] },
        "status": { "color": "#a875d1", "values": [] },
    })
}

/// Each entry must be an object with a string `color` and an array-of-strings
/// `values`; `single_value`, if present, must be a bool. Malformed entries
/// are rejected outright rather than silently persisted — a bad write here
/// previously corrupted the registry until the next `unwrap_or_else` reset.
fn validate_tag_namespaces(body: &Value) -> Result<(), String> {
    let obj = body
        .as_object()
        .ok_or_else(|| "tag settings must be a JSON object".to_string())?;
    for (ns, entry) in obj {
        let entry_obj = entry
            .as_object()
            .ok_or_else(|| format!("namespace {ns:?} must be an object"))?;
        match entry_obj.get("color") {
            Some(Value::String(_)) => {}
            _ => return Err(format!("namespace {ns:?} missing string \"color\"")),
        }
        match entry_obj.get("values") {
            Some(Value::Array(vals)) if vals.iter().all(|v| v.is_string()) => {}
            _ => return Err(format!("namespace {ns:?} missing string array \"values\"")),
        }
        if let Some(sv) = entry_obj.get("single_value") {
            if !sv.is_boolean() {
                return Err(format!("namespace {ns:?} \"single_value\" must be a bool"));
            }
        }
    }
    Ok(())
}

pub(super) async fn get_tag_settings(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    let raw = store.get_meta("tag_namespaces").await?;
    let registry = match raw {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| default_tag_namespaces()),
        None => default_tag_namespaces(),
    };
    Ok(Json(registry))
}

pub(super) async fn save_tag_settings(
    State(store): State<Store>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_tag_namespaces(&body).map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    store.set_meta("tag_namespaces", &body.to_string()).await?;
    Ok(Json(json!({ "saved": true })))
}
