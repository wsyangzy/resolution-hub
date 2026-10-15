fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-light".into());
    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("failed to compile Slint UI");

    println!("cargo:rerun-if-changed=../assets/resolution-hub.ico");
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/resolution-hub.ico")
        .to_string_lossy()
        .replace('\\', "/");
    let version = env!("CARGO_PKG_VERSION");
    let parts: Vec<_> = version.split('.').collect();
    let resource = format!(
        r#"#include <windows.h>
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "FileDescription", "Resolution Hub"
            VALUE "ProductName", "Resolution Hub"
            VALUE "FileVersion", "{version}"
            VALUE "ProductVersion", "{version}"
            VALUE "OriginalFilename", "resolution-hub.exe"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#,
        major = parts[0],
        minor = parts[1],
        patch = parts[2],
    );
    let resource_path =
        std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("resolution-hub.rc");
    std::fs::write(&resource_path, resource).expect("failed to write Windows resources");
    embed_resource::compile(&resource_path, embed_resource::NONE)
        .manifest_required()
        .expect("failed to compile Windows resources");

    // 独立 EXE 保留 Common-Controls v6 清单，不再依靠 Tauri 的应用清单。
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}
