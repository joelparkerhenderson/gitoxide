use crate::remote::find;
use crate::{remote, Remote};
use std::convert::TryInto;

impl crate::Repository {
    /// Create a new remote available at the given `url`.
    pub fn remote_at<Url, E>(&self, url: Url) -> Result<Remote<'_>, remote::init::Error>
    where
        Url: TryInto<git_url::Url, Error = E>,
        git_url::parse::Error: From<E>,
    {
        Remote::from_fetch_url(url, true, self)
    }

    /// Create a new remote available at the given `url`, but don't rewrite the url according to rewrite rules.
    /// This eliminates a failure mode in case the rewritten URL is faulty, allowing to selectively [apply rewrite
    /// rules][Remote::rewrite_urls()] later and do so non-destructively.
    pub fn remote_at_without_url_rewrite<Url, E>(&self, url: Url) -> Result<Remote<'_>, remote::init::Error>
    where
        Url: TryInto<git_url::Url, Error = E>,
        git_url::parse::Error: From<E>,
    {
        Remote::from_fetch_url(url, false, self)
    }

    /// Find the remote with the given `name` or report an error, similar to [`try_find_remote(…)`][Self::try_find_remote()].
    ///
    /// Note that we will include remotes only if we deem them [trustworthy][crate::open::Options::filter_config_section()].
    pub fn find_remote(&self, name: &str) -> Result<Remote<'_>, find::existing::Error> {
        Ok(self
            .try_find_remote(name)
            .ok_or_else(|| find::existing::Error::NotFound { name: name.into() })??)
    }

    /// Find the remote with the given `name` or return `None` if it doesn't exist, for the purpose of fetching or pushing
    /// data to a remote.
    ///
    /// There are various error kinds related to partial information or incorrectly formatted URLs or ref-specs.
    /// Also note that the created `Remote` may have neither fetch nor push ref-specs set at all.
    ///
    /// Note that ref-specs are de-duplicated right away which may change their order. This doesn't affect matching in any way
    /// as negations/excludes are applied after includes.
    ///
    /// We will only include information if we deem it [trustworthy][crate::open::Options::filter_config_section()].
    pub fn try_find_remote(&self, name: &str) -> Option<Result<Remote<'_>, find::Error>> {
        self.try_find_remote_inner(name, true)
    }

    /// Similar to [try_find_remote()][Self::try_find_remote()], but removes a failure mode if rewritten URLs turn out to be invalid
    /// as it skips rewriting them.
    /// Use this in conjunction with [`Remote::rewrite_urls()`] to non-destructively apply the rules and keep the failed urls unchanged.
    pub fn try_find_remote_without_url_rewrite(&self, name: &str) -> Option<Result<Remote<'_>, find::Error>> {
        self.try_find_remote_inner(name, false)
    }

    fn try_find_remote_inner(&self, name: &str, rewrite_urls: bool) -> Option<Result<Remote<'_>, find::Error>> {
        let mut filter = self.filter_config_section();
        let mut config_url = |field: &str, kind: &'static str| {
            self.config
                .resolved
                .string_filter("remote", name.into(), field, &mut filter)
                .map(|url| {
                    git_url::parse::parse(url.as_ref()).map_err(|err| find::Error::Url {
                        kind,
                        url: url.into_owned(),
                        source: err,
                    })
                })
        };
        let url = config_url("url", "fetch");
        let push_url = config_url("pushUrl", "push");

        let mut config_spec = |op: git_refspec::parse::Operation| {
            let kind = match op {
                git_refspec::parse::Operation::Fetch => "fetch",
                git_refspec::parse::Operation::Push => "push",
            };
            self.config
                .resolved
                .strings_filter("remote", name.into(), kind, &mut filter)
                .map(|specs| {
                    specs
                        .into_iter()
                        .map(|spec| {
                            git_refspec::parse(spec.as_ref(), op)
                                .map(|spec| spec.to_owned())
                                .map_err(|err| find::Error::RefSpec {
                                    spec: spec.into_owned(),
                                    kind,
                                    source: err,
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(|mut specs| {
                            specs.sort();
                            specs.dedup();
                            specs
                        })
                })
        };
        let fetch_specs = config_spec(git_refspec::parse::Operation::Fetch);
        let push_specs = config_spec(git_refspec::parse::Operation::Push);

        match (url, fetch_specs, push_url, push_specs) {
            (None, None, None, None) => None,
            (None, _, None, _) => Some(Err(find::Error::UrlMissing)),
            (url, fetch_specs, push_url, push_specs) => {
                let url = match url {
                    Some(Ok(v)) => Some(v),
                    Some(Err(err)) => return Some(Err(err)),
                    None => None,
                };
                let push_url = match push_url {
                    Some(Ok(v)) => Some(v),
                    Some(Err(err)) => return Some(Err(err)),
                    None => None,
                };
                let fetch_specs = match fetch_specs {
                    Some(Ok(v)) => v,
                    Some(Err(err)) => return Some(Err(err)),
                    None => Vec::new(),
                };
                let push_specs = match push_specs {
                    Some(Ok(v)) => v,
                    Some(Err(err)) => return Some(Err(err)),
                    None => Vec::new(),
                };

                Some(
                    Remote::from_preparsed_config(
                        name.to_owned().into(),
                        url,
                        push_url,
                        fetch_specs,
                        push_specs,
                        rewrite_urls,
                        self,
                    )
                    .map_err(Into::into),
                )
            }
        }
    }
}
