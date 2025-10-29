use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::exit,
};

use clap::Parser;
use include_dir::{Dir, include_dir};
use itertools::Itertools;

static DATA: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/data");

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    match args.subcommand {
        Subcommand::Gen {
            wasm_file,
            out_dir,
            is_precompiled,
        } => {
            if is_precompiled {
                for e in std::fs::read_dir(wasm_file)? {
                    let e = e?;
                    if !(e.file_name().as_encoded_bytes().ends_with(b".c")
                        || e.file_name().as_encoded_bytes().ends_with(b".h"))
                        || !(e.file_name().as_encoded_bytes().starts_with(b"wasm"))
                    {
                        continue;
                    }
                    std::fs::copy(e.path(), out_dir.join(Path::new(&e.file_name())))?;
                }
            } else {
                let s = std::process::Command::new("w2c2")
                    .arg("-f")
                    .arg("256")
                    .arg(wasm_file)
                    .arg(out_dir.join(Path::new("wasm.c")))
                    .spawn()?
                    .wait()?;
                if !s.success() {
                    exit(s.code().unwrap());
                }
            }
            let mut sources = BTreeSet::new();
            let mut headers = BTreeSet::new();
            for e in std::fs::read_dir(&out_dir)? {
                let e = e?;
                if !(e.file_name().as_encoded_bytes().starts_with(b"wasm")) {
                    continue;
                }

                if e.file_name().as_encoded_bytes().ends_with(b".c") {
                    sources.insert(e.path().strip_prefix(&out_dir).unwrap().to_owned());
                }
                if e.file_name().as_encoded_bytes().ends_with(b".h") {
                    headers.insert(e.path().strip_prefix(&out_dir).unwrap().to_owned());
                }
            }
            for e in DATA.files() {
                std::fs::write(out_dir.join(e.path()), e.contents())?;
                if e.path().as_os_str().as_encoded_bytes().ends_with(b".c") {
                    sources.insert(e.path().to_owned());
                }
                if e.path().as_os_str().as_encoded_bytes().ends_with(b".h") {
                    headers.insert(e.path().to_owned());
                }
            }
            std::fs::write(out_dir.join("main.c"), format!(r#"
            #include <stdio.h>
            #include "w2c2_base.h"
            #include "wasi.h"
            #include "wasm.h"
            #include <unistd.h>


            void
            trap(
                Trap trap
            ) {{
                fprintf(stderr, "TRAP: %s\n", trapDescription(trap));
                abort();
            }}

            wasmMemory*
            wasiMemory(
                void* instance
            ) {{
                return wasm_memory((wasmInstance*)instance);
            }}

            
            extern char** environ;
            

            /* Main */

            int main(int argc, char* argv[]) {{

                char buf[PATH_MAX];
                getcwd(buf,sizeof(buf));

                setenv("PWD",buf);

                /* Initialize WASI */
                if (!wasiInit(argc, argv, environ)) {{
                    fprintf(stderr, "failed to init WASI\n");
                    return 1;
                }}


                if (!wasiFileDescriptorAdd(-1, buf, NULL)) {{
                    fprintf(stderr, "failed to add preopen\n");
                    return 1;
                }}


                {{
                    wasmInstance instance;
                    wasmInstantiate(&instance, NULL);
                    wasm__start(&instance);
                    wasmFreeInstance(&instance);
                }}

                return 0;
            }}
            "#))?;
            sources.insert(PathBuf::from("main.c"));
            let sources = sources
                .iter()
                .filter_map(|a| Some(format!("\"{}\"", a.to_str()?)))
                .join(",");
            let headers = headers
                .iter()
                .filter_map(|a| Some(format!("\"{}\"", a.to_str()?)))
                .join(",");
            std::fs::write(
                out_dir.join("BUILD.bazel"),
                format!(
                    "
            cc_binary(
            name = \"wasm\",
            srcs = [{sources}],
            hdrs = [{headers}]
            )
            ",
                ),
            )?;
            std::fs::write(
                out_dir.join("BUCK"),
                format!(
                    "
            cxx_binary(
            name = \"wasm\",
            srcs = [{sources}],
            headers = [{headers}]
            )
            ",
                ),
            )?;
        }
    };
    Ok(())
}
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: Subcommand,
}
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Subcommand {
    Gen {
        ///The wasm file to compile if not precompiled, else the directory containing a `w2c2` output
        wasm_file: PathBuf,
        out_dir: PathBuf,
        #[arg(name = "precompiled")]
        is_precompiled: bool,
    },
}
