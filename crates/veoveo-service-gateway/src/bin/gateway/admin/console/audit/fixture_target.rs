//! A fixture owner Audit target. Audit tests use it to exercise owner registration
//! without depending on any domain crate.
use veoveo_audit_contract::{
    AuditLookupKey, AuditLookupReference, AuditTargetError, AuditTargetOwner,
    AuditTargetRegistration, AuditTargetRegistryBuilder,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct SensorId(uuid::Uuid);

impl SensorId {
    #[allow(
        dead_code,
        reason = "Each audit test selects fresh or literal identities"
    )]
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    #[allow(
        dead_code,
        reason = "Each audit test selects fresh or literal identities"
    )]
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        uuid::Uuid::parse_str(value).map(Self)
    }

    #[allow(dead_code, reason = "Only lookup assertions read the UUID")]
    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

impl std::fmt::Display for SensorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, schemars::JsonSchema)]
#[schemars(transform = audit_target_schema)]
#[schemars(deny_unknown_fields)]
pub struct SensorAuditTarget {
    pub sensor: SensorId,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum SensorTargetWire {
    #[serde(rename = "sensor")]
    Sensor { sensor: SensorId },
}

impl serde::Serialize for SensorAuditTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(
            &SensorTargetWire::Sensor {
                sensor: self.sensor,
            },
            serializer,
        )
    }
}

impl<'de> serde::Deserialize<'de> for SensorAuditTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match <SensorTargetWire as serde::Deserialize>::deserialize(deserializer)? {
            SensorTargetWire::Sensor { sensor } => Ok(Self { sensor }),
        }
    }
}

impl AuditTargetOwner for SensorAuditTarget {
    const KIND: &'static str = "sensor";
    fn lookup_reference(&self) -> Result<Option<AuditLookupReference>, AuditTargetError> {
        Ok(Some(AuditLookupReference::new(
            "sensor",
            AuditLookupKey::Uuid(self.sensor.0),
        )?))
    }
}

pub fn register_audit_target(
    builder: &mut AuditTargetRegistryBuilder,
) -> Result<AuditTargetRegistration<SensorAuditTarget>, AuditTargetError> {
    builder.register::<SensorAuditTarget>()
}

fn audit_target_schema(schema: &mut schemars::Schema) {
    let object = schema.as_object_mut().expect("Sensor target object");
    object
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .expect("Sensor target properties")
        .insert(
            "kind".into(),
            serde_json::json!({"type":"string","const":"sensor"}),
        );
    object
        .get_mut("required")
        .and_then(serde_json::Value::as_array_mut)
        .expect("Sensor target required fields")
        .push(serde_json::json!("kind"));
}
