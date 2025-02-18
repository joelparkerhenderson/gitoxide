///
pub mod set_target_id {
    use crate::bstr::BString;
    use crate::Reference;
    use git_ref::transaction::PreviousValue;
    use git_ref::Target;

    mod error {
        use git_ref::FullName;

        /// The error returned by [`Reference::set_target_id()`][super::Reference::set_target_id()].
        #[derive(Debug, thiserror::Error)]
        #[allow(missing_docs)]
        pub enum Error {
            #[error("Cannot change symbolic reference {name:?} into a direct one by setting it to an id")]
            SymbolicReference { name: FullName },
            #[error(transparent)]
            ReferenceEdit(#[from] crate::reference::edit::Error),
        }
    }
    pub use error::Error;

    impl<'repo> Reference<'repo> {
        /// Set the id of this direct reference to `id` and use `reflog_message` for the reflog (if enabled in the repository).
        ///
        /// Note that the operation will fail on symbolic references, to change their type use the lower level reference database,
        /// or if the reference was deleted or changed in the mean time.
        /// Furthermore, refrain from using this method for more than a one-off change as it creates a transaction for each invocation.
        /// If multiple reference should be changed, use [Repository::edit_references()][crate::Repository::edit_references()]
        /// or the lower level reference database instead.
        pub fn set_target_id(
            &mut self,
            id: impl Into<git_hash::ObjectId>,
            reflog_message: impl Into<BString>,
        ) -> Result<(), Error> {
            match &self.inner.target {
                Target::Symbolic(name) => return Err(Error::SymbolicReference { name: name.clone() }),
                Target::Peeled(current_id) => {
                    let changed = self.repo.reference(
                        self.name(),
                        id,
                        PreviousValue::MustExistAndMatch(Target::Peeled(current_id.to_owned())),
                        reflog_message,
                    )?;
                    *self = changed;
                }
            }
            Ok(())
        }
    }
}

///
pub mod delete {
    use crate::Reference;
    use git_ref::transaction::{Change, PreviousValue, RefEdit, RefLog};

    mod error {
        /// The error returned by [`Reference::delete()`][super::Reference::delete()].
        #[derive(Debug, thiserror::Error)]
        #[allow(missing_docs)]
        pub enum Error {
            #[error(transparent)]
            ReferenceEdit(#[from] crate::reference::edit::Error),
        }
    }
    pub use error::Error;
    use git_lock::acquire::Fail;

    impl<'repo> Reference<'repo> {
        /// Delete this reference or fail if it was changed since last observed.
        /// Note that this instance remains available in memory but probably shouldn't be used anymore.
        pub fn delete(&self) -> Result<(), Error> {
            self.repo.edit_reference(
                RefEdit {
                    change: Change::Delete {
                        expected: PreviousValue::MustExistAndMatch(self.inner.target.clone()),
                        log: RefLog::AndReference,
                    },
                    name: self.inner.name.clone(),
                    deref: false,
                },
                Fail::Immediately,
                self.repo.committer_or_default(),
            )?;
            Ok(())
        }
    }
}
