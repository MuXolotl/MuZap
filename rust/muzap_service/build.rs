use std::io;
use winresource::WindowsResource;

fn main() -> io::Result<()> {
    // Для службы не нужен requireAdministrator. Она запускается SCM под системной учеткой.
    // Но полезно иметь явный манифест без UAC-эскалации при ручном запуске.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = WindowsResource::new();

        res.set_manifest(
            r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
        );

        res.compile()?;
    }

    Ok(())
}
