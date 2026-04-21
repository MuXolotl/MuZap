use std::io;
use winresource::WindowsResource;

fn main() -> io::Result<()> {
    // Встраиваем манифест requireAdministrator, чтобы Windows сама показала UAC корректно.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = WindowsResource::new();

        res.set_manifest(
            r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
        );

        // res.set_icon("icon.ico");
        res.compile()?;
    }

    Ok(())
}
