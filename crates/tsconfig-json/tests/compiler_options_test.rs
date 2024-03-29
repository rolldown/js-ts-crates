use typescript_tsconfig_json::{
    CompilerOptions, JsxField, ModuleField, ModuleResolutionField, TargetField,
};

#[test]
fn handles_jsx() {
    let options: CompilerOptions = serde_json::from_str(r#"{ "jsx": "ReactJsxdev" }"#).unwrap();

    assert_eq!(options.jsx.unwrap(), JsxField::ReactJsxdev);

    let options: CompilerOptions = serde_json::from_str(r#"{ "jsx": "react-jsxdev" }"#).unwrap();

    assert_eq!(options.jsx.unwrap(), JsxField::ReactJsxdev);

    assert_eq!(
        serde_json::to_string(&JsxField::ReactJsxdev).unwrap(),
        "\"react-jsxdev\""
    );
}

#[test]
fn handles_module() {
    let options: CompilerOptions = serde_json::from_str(r#"{ "module": "es2015" }"#).unwrap();

    assert_eq!(options.module.unwrap(), ModuleField::Es2015);

    let options: CompilerOptions = serde_json::from_str(r#"{ "module": "Es2015" }"#).unwrap();

    assert_eq!(options.module.unwrap(), ModuleField::Es2015);

    assert_eq!(
        serde_json::to_string(&ModuleField::Es2015).unwrap(),
        "\"es2015\""
    );
}

#[test]
fn handles_module_resolution() {
    let options: CompilerOptions =
        serde_json::from_str(r#"{ "moduleResolution": "nodenext" }"#).unwrap();

    assert_eq!(
        options.module_resolution.unwrap(),
        ModuleResolutionField::NodeNext
    );

    let options: CompilerOptions =
        serde_json::from_str(r#"{ "moduleResolution": "NodeNext" }"#).unwrap();

    assert_eq!(
        options.module_resolution.unwrap(),
        ModuleResolutionField::NodeNext
    );

    assert_eq!(
        serde_json::to_string(&ModuleResolutionField::NodeNext).unwrap(),
        "\"nodenext\""
    );
}

#[test]
fn handles_target() {
    let options: CompilerOptions = serde_json::from_str(r#"{ "target": "esnext" }"#).unwrap();

    assert_eq!(options.target.unwrap(), TargetField::EsNext);

    let options: CompilerOptions = serde_json::from_str(r#"{ "target": "EsNext" }"#).unwrap();

    assert_eq!(options.target.unwrap(), TargetField::EsNext);

    assert_eq!(
        serde_json::to_string(&TargetField::EsNext).unwrap(),
        "\"esnext\""
    );
}
