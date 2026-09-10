# Reader clients

A **Client Package** is an Aidoku `.aix` or Tachiyomi/Mihon APK served by `/v1/clients/{client}/package`. Runtime file overrides take precedence, followed by packages embedded during the server build, followed by an on-demand build from the checkout. Each client has one build lock that remains held until its build process finishes, including when the requesting HTTP connection is cancelled.

**Client Authentication** uses the server's bearer API key. Reader clients do not participate in the browser OIDC flow. Their server address must be reachable from the device.

Both clients consume the shared library, source and page APIs. Downloaded chapter pages default to the best available variant; original bytes remain available explicitly. Aidoku retains its own reading state, while the web application records server reading progress.

Aidoku has an isolated Cargo workspace and a pinned official SDK. The APK compiles Kotlin against compile-only Java stubs, then uses Android build tools to package, align, sign and verify it. Build and package validation are separate from installing and reading pages in the host application.
