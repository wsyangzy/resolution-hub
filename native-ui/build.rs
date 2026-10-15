fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-light".into());
    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("failed to compile Slint UI");

    // 复用后端带入了 muda 的 TaskDialogIndirect 导入；依赖库的清单不会传给独立 EXE。
    // MSVC 必须把 Common-Controls v6 的依赖嵌入这个应用，供 Windows 加载器选择正确的 DLL。
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}
