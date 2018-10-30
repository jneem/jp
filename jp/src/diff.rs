use clap::ArgMatches;
use diff::LineDiff;
use failure::Error;
use libjp::Repo;
use std::io::prelude::*;

// TODO: we should refactor some of this into libjp. In particular, it's probably useful to
// have a method for taking a file and producing a diff.
pub fn diff(repo: &Repo) -> Result<Vec<LineDiff>, Error> {
    let mut fs_file = repo.open_file()?;
    let mut fs_file_contents = Vec::new();
    fs_file.read_to_end(&mut fs_file_contents)?;
    let fs_lines = lines(&fs_file_contents);

    if let Some(repo_file) = repo.file("master") {
        let repo_lines = (0..repo_file.num_lines())
            .map(|i| repo_file.line(i).to_owned())
            .collect::<Vec<_>>();
        Ok(diff::diff(&repo_lines, &fs_lines))
    } else {
        panic!("FIXME");
    }
}

pub fn run(_m: &ArgMatches) -> Result<(), Error> {
    let repo = super::open_repo()?;

    let diff = diff(&repo)?;
    println!("{:?}", diff);

    Ok(())
}

// Splits a file into \n-separated lines. This differs from the method in the standard library in
// that it keeps the line endings.
// TODO: we should (after benchmarking) revisit how we're comparing files. For example, it might be
// worth interning things for quicker comparisons, and possibly reduced allocations.
fn lines(input: &[u8]) -> Vec<Vec<u8>> {
    let mut ret = Vec::new();
    let mut cur_idx = 0;
    for (newline_idx, _) in input.into_iter().enumerate().filter(|&(_, &b)| b == b'\n') {
        ret.push(input[cur_idx..=newline_idx].to_owned());
        cur_idx = newline_idx + 1;
    }
    if input.is_empty() || input.last().unwrap() != &b'\n' {
        ret.push(input[cur_idx..].to_owned());
    }
    ret
}

#[cfg(test)]
mod tests {
    #[test]
    fn lines() {
        assert_eq!(super::lines(&b""[..]), vec![Vec::<u8>::new()]);
        assert_eq!(super::lines(&b"\n"[..]), vec![b"\n".to_owned()]);
        assert_eq!(
            super::lines(&b"a\nb\n"[..]),
            vec![b"a\n".to_owned(), b"b\n".to_owned()]
        );
        assert_eq!(
            super::lines(&b"a\nb"[..]),
            vec![b"a\n"[..].to_owned(), b"b"[..].to_owned()]
        );
    }
}
