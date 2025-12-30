pub mod branch_picker;

pub use branch_picker::{
    checkout_remote, create_branch, gather_branch_data, switch_branch, BranchPickerData,
    LocalBranch, RemoteBranch,
};
