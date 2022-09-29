use std::io::prelude::*;
use std::io::BufReader;
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;

use log::debug;
use fluvio::ConsumerConfig;
use fluvio::consumer::{SmartModuleInvocation, SmartModuleKind, SmartModuleInvocationWasm};

use flate2::write::GzEncoder;
use flate2::Compression;

use node_bindgen::core::NjError;
use node_bindgen::core::JSValue;
use node_bindgen::core::val::JsEnv;
use node_bindgen::sys::napi_value;
use node_bindgen::core::val::JsObject;

use crate::{optional_property, must_property};

const CONFIG_SMART_MODULE_MAX_BYTES_KEY: &str = "maxBytes";
const CONFIG_SMART_MODULE_DATA_KEY: &str = "smartmoduleData";
const CONFIG_SMART_MODULE_TYPE_KEY: &str = "smartmoduleType";
const CONFIG_SMART_MODULE_NAME_KEY: &str = "smartmoduleName";
const CONFIG_SMART_MODULE_FILE_KEY: &str = "smartmoduleFile";

pub struct ConfigWrapper {
    pub inner: ConsumerConfig,
}

impl JSValue<'_> for ConfigWrapper {
    fn convert_to_rust(env: &JsEnv, js_value: napi_value) -> Result<Self, NjError> {
        debug!("convert fetch consumer config param");
        if let Ok(js_obj) = env.convert_to_rust::<JsObject>(js_value) {
            let smartmodule_type = must_property!(CONFIG_SMART_MODULE_TYPE_KEY, String, js_obj);
            let smartmodule_file = optional_property!(CONFIG_SMART_MODULE_FILE_KEY, String, js_obj);
            let smartmodule_name = optional_property!(CONFIG_SMART_MODULE_NAME_KEY, String, js_obj);
            let smartmodule_data = optional_property!(CONFIG_SMART_MODULE_DATA_KEY, String, js_obj);
            let kind = match smartmodule_type.as_str() {
                "filter" => Ok(SmartModuleKind::Filter),
                "map" => Ok(SmartModuleKind::Map),
                "array_map" => Ok(SmartModuleKind::ArrayMap),
                "filter_map" => Ok(SmartModuleKind::FilterMap),
                _ => Err(NjError::Other(format!(
                    "Provided SmartModule type: \"{}\" is not valid",
                    smartmodule_type
                ))),
            }?;

            let mut config_builder = ConsumerConfig::builder();
            let smartmodule: Option<SmartModuleInvocation> =
                match (smartmodule_file, smartmodule_name, smartmodule_data) {
                    (None, None, None) => Ok(None),
                    (Some(file), None, None) => {
                        let path = PathBuf::from_str(file.as_str())
                            .map_err(|e| NjError::Other(e.to_string()))?;
                        let file = File::open(path).unwrap();
                        let reader = BufReader::new(file);
                        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::default());

                        gz_encoder.write_all(reader.buffer()).unwrap();
                        let bytes = gz_encoder.finish().unwrap();

                        Ok(Some(SmartModuleInvocation {
                            wasm: SmartModuleInvocationWasm::AdHoc(bytes),
                            kind,
                            ..Default::default()
                        }))
                    }
                    (None, Some(name), None) => Ok(Some(SmartModuleInvocation {
                        wasm: SmartModuleInvocationWasm::Predefined(name),
                        kind,
                        ..Default::default()
                    })),
                    (None, None, Some(data)) => {
                        let wasm = base64::decode(data).map_err(|e| {
                            NjError::Other(format!(
                "An error ocurred attempting to decode the Base64 WASM file provided. {:?}",
                e
            ))
                        })?;

                        Ok(Some(SmartModuleInvocation {
                            wasm: SmartModuleInvocationWasm::AdHoc(wasm),
                            kind,
                            ..Default::default()
                        }))
                    }
                    _ => Err(NjError::Other(format!(
                        "You must either provide one of {}, {} or {}",
                        CONFIG_SMART_MODULE_FILE_KEY,
                        CONFIG_SMART_MODULE_NAME_KEY,
                        CONFIG_SMART_MODULE_DATA_KEY
                    ))),
                }?;

            config_builder.smartmodule(smartmodule);

            if let Some(max_bytes) =
                optional_property!(CONFIG_SMART_MODULE_MAX_BYTES_KEY, i32, js_obj)
            {
                config_builder.max_bytes(max_bytes);
            };

            let consumer_config = config_builder
                .build()
                .map_err(|e| NjError::Other(e.to_string()))?;

            return Ok(Self {
                inner: consumer_config,
            });
        }

        Err(NjError::Other("must pass json param".to_owned()))
    }
}
