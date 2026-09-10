use wasmtime::component::bindgen;

bindgen!({
    path: "wit/manga-source.wit",
    world: "manga-source",
});
