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

/// Each entry must be an object with a string `color` and an array-of-strings
/// `values`; `single_value` and `description`, if present, must be a bool
/// and a string respectively; `values_mode`, if present, must be the string
/// "suggestion" or "fixed" — "fixed" makes `values` an enforced allow-list
/// (checked in `store::validate_tags_against_registry`), "suggestion"
/// (the default when absent) keeps it purely advisory. Malformed entries
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
        if let Some(sv) = entry_obj.get("single_value")
            && !sv.is_boolean()
        {
            return Err(format!("namespace {ns:?} \"single_value\" must be a bool"));
        }
        if let Some(d) = entry_obj.get("description")
            && !d.is_string()
        {
            return Err(format!("namespace {ns:?} \"description\" must be a string"));
        }
        if let Some(vm) = entry_obj.get("values_mode")
            && vm.as_str() != Some("suggestion")
            && vm.as_str() != Some("fixed")
        {
            return Err(format!(
                "namespace {ns:?} \"values_mode\" must be \"suggestion\" or \"fixed\""
            ));
        }
    }
    Ok(())
}

pub(super) async fn get_tag_settings(State(store): State<Store>) -> Result<Json<Value>, ApiError> {
    Ok(Json(store.tag_namespace_registry().await))
}

pub(super) async fn save_tag_settings(
    State(store): State<Store>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_tag_namespaces(&body).map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    store.set_meta("tag_namespaces", &body.to_string()).await?;
    Ok(Json(json!({ "saved": true })))
}
