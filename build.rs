use std::{
    fs,
    io::{BufRead, BufReader, Result},
};

const LIB_RS: &str = "src/lib.rs";
const END_OF_README: &str = "// <END_OF_README>";

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=src");

    let mut index_with_readme = {
        let mut readme = String::new();
        for line in BufReader::new(fs::File::open("README.md")?).lines() {
            if let Ok(doc) = line {
                readme.push_str(&["//!", &doc, "\n"].concat());
            }
        }

        readme.push_str(END_OF_README);
        readme
    };

    let index = fs::read_to_string(LIB_RS)?;
    let end = index.find(END_OF_README).expect("No README in lib.rs");

    index_with_readme.push_str(&index[end + END_OF_README.len()..]);
    fs::write(LIB_RS, index_with_readme)?;
    Ok(())
}
